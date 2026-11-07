#!/usr/bin/env python3
"""Start the desktop application on a hidden X display and check that it paints.

The application is started with its automation bridge, asked for a screenshot,
and the image is measured: a renderer that opens a blank (all-white or
all-black) client area fails here.  The window is then resized and measured
again, and closed through the window manager so a clean shutdown is part of the
check.

Run it under Xvfb (the script starts a window manager of its own):

    xvfb-run -a -s "-screen 0 1280x800x24" \\
        python3 scripts/linux-gui-smoke.py --exe target/release/FreePlots

The default Linux backend is glow; the wgpu backend is optional, like glow on a
Windows runner without OpenGL: it must paint or exit with a clear error, never
hang and never stay blank.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path

STARTUP_TIMEOUT = 180
CLOSE_TIMEOUT = 20


class Failure(Exception):
    """A check that did not pass."""


class Bridge:
    """One JSON command per line over a socket to the running window."""

    def __init__(self, connection: socket.socket) -> None:
        connection.settimeout(60)
        self._stream = connection.makefile("rwb")

    def call(self, request: dict) -> dict:
        self._stream.write((json.dumps(request) + "\n").encode("utf-8"))
        self._stream.flush()
        line = self._stream.readline()
        if not line:
            raise Failure("the bridge closed the connection")
        return json.loads(line)

    def close(self) -> None:
        self._stream.close()


class App:
    """A running application process and its log."""

    def __init__(
        self, exe: str, port: int, extra: list[str], env: dict[str, str], log_dir: Path
    ) -> None:
        self.log = log_dir / "app.log"
        self._handle = open(self.log, "wb")
        environment = dict(os.environ)
        environment.pop("FREEPLOTS_RENDERER", None)
        environment.update(env)
        self.process = subprocess.Popen(
            [exe, "--gui", "--bridge", f"127.0.0.1:{port}", *extra],
            stdout=self._handle,
            stderr=subprocess.STDOUT,
            env=environment,
        )

    def text(self) -> str:
        try:
            return self.log.read_text(encoding="utf-8", errors="replace")
        except OSError:
            return ""

    def close_log(self) -> None:
        self._handle.close()

    def stop(self) -> None:
        if self.process.poll() is None:
            self.process.kill()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
        self.close_log()


def free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def connect(app: App, port: int, timeout: int) -> Bridge | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if app.process.poll() is not None:
            return None
        try:
            return Bridge(socket.create_connection(("127.0.0.1", port), timeout=1))
        except OSError:
            time.sleep(0.5)
    return None


def window_id(name: str) -> str | None:
    """The visible X window whose title starts with `name`, if there is one."""
    found = subprocess.run(
        ["xdotool", "search", "--onlyvisible", "--name", f"^{name}"],
        capture_output=True,
        text=True,
        check=False,
    )
    ids = found.stdout.split()
    return ids[0] if ids else None


def screenshot(bridge: Bridge, path: Path) -> None:
    path.unlink(missing_ok=True)
    reply = bridge.call({"cmd": "screenshot", "path": str(path)})
    if not reply.get("ok"):
        raise Failure(f"screenshot command failed: {reply.get('error')}")
    if not path.exists():
        raise Failure(f"the screenshot was not written to {path}")


def measure(path: Path) -> dict:
    try:
        from PIL import Image
    except ImportError as error:  # pragma: no cover - the runner installs it
        raise Failure(f"Pillow is required to inspect the screenshot: {error}") from error

    image = Image.open(path).convert("RGB")
    width, height = image.size
    step_x = max(1, width // 180)
    step_y = max(1, height // 120)
    colours: Counter[tuple[int, int, int]] = Counter()
    ink = 0
    samples = 0
    pixels = image.load()
    for y in range(0, height, step_y):
        for x in range(0, width, step_x):
            colour = pixels[x, y]
            colours[colour] += 1
            luminance = 0.299 * colour[0] + 0.587 * colour[1] + 0.114 * colour[2]
            if luminance < 240:
                ink += 1
            samples += 1
    top = colours.most_common(1)[0][1]
    return {
        "width": width,
        "height": height,
        "colours": len(colours),
        "top_share": top / samples,
        "ink": ink / samples,
        "bytes": path.stat().st_size,
    }


def assert_rendered(shot: dict, label: str) -> None:
    ink = round(shot["ink"] * 100, 1)
    top = round(shot["top_share"] * 100, 1)
    if shot["bytes"] < shot["width"] * shot["height"] * 2:
        raise Failure(
            f"{label}: the screenshot is only {shot['bytes']} bytes for "
            f"{shot['width']}x{shot['height']}"
        )
    if shot["colours"] < 16:
        raise Failure(f"{label}: only {shot['colours']} distinct colours (the window is blank)")
    if shot["top_share"] > 0.995:
        raise Failure(f"{label}: one colour covers {top}% of the window (the window is blank)")
    if shot["ink"] < 0.005:
        raise Failure(f"{label}: only {ink}% of the window is drawn (the window is blank)")
    if shot["ink"] > 0.98:
        raise Failure(f"{label}: {ink}% of the window is drawn (the window is a solid block)")
    print(
        f"  PASS  {label} painted {shot['width']}x{shot['height']}, "
        f"{shot['colours']} colours, {ink}% ink, {shot['bytes']} bytes"
    )


def close_cleanly(app: App, window: str | None, label: str) -> None:
    if window:
        subprocess.run(["xdotool", "windowactivate", "--sync", window], check=False)
        subprocess.run(["xdotool", "key", "--clearmodifiers", "alt+F4"], check=False)
    try:
        code = app.process.wait(timeout=CLOSE_TIMEOUT)
    except subprocess.TimeoutExpired:
        raise Failure(f"{label}: the application did not close within {CLOSE_TIMEOUT} s") from None
    if code != 0:
        raise Failure(f"{label}: exit code {code}\n{app.text()}")
    print(f"  PASS  {label} closed cleanly (exit 0)")


def run_case(
    name: str,
    exe: str,
    extra: list[str],
    env: dict[str, str],
    resize: bool,
    optional: bool,
    temp: Path,
) -> None:
    print(f"=== {name}")
    port = free_port()
    case_dir = temp / name.replace(" ", "-")
    case_dir.mkdir(parents=True, exist_ok=True)
    app = App(exe, port, extra, env, case_dir)
    bridge = None
    window = None
    try:
        bridge = connect(app, port, STARTUP_TIMEOUT if not optional else 90)
        if bridge is None:
            if optional and app.process.poll() is not None:
                code = app.process.returncode
                log = app.text().strip()
                if code < 0:
                    raise Failure(f"{name}: crashed with signal {-code}\n{log}")
                if code != 0 and log:
                    print(f"  PASS  {name} is unavailable here (exit {code}): {log.splitlines()[-1]}")
                    return
                raise Failure(f"{name}: exit code {code} without an explanation\n{log}")
            raise Failure(
                f"{name}: no bridge on 127.0.0.1:{port} within the startup timeout\n{app.text()}"
            )
        shot_path = case_dir / "shot.png"
        screenshot(bridge, shot_path)
        assert_rendered(measure(shot_path), name)

        window = window_id("FreePlots")
        if window is None:
            raise Failure(f"{name}: no visible window titled FreePlots")
        if resize:
            subprocess.run(["xdotool", "windowsize", window, "900", "640"], check=False)
            time.sleep(2)
            resized_path = case_dir / "resized.png"
            screenshot(bridge, resized_path)
            resized = measure(resized_path)
            assert_rendered(resized, f"{name} (resized)")
            if abs(resized["width"] - 900) > 60:
                raise Failure(
                    f"{name}: the window reports {resized['width']} px wide after a resize to 900 px"
                )
            print(f"  PASS  {name} resized to {resized['width']}x{resized['height']}")
        close_cleanly(app, window, name)
    finally:
        if bridge is not None:
            bridge.close()
        if app.process.poll() is None:
            app.stop()
        else:
            app.close_log()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exe", required=True, help="FreePlots executable to run")
    parser.add_argument("--keep", action="store_true", help="keep the screenshots and logs")
    args = parser.parse_args()

    exe = str(Path(args.exe).resolve())
    if not Path(exe).exists():
        print(f"FreePlots executable not found: {exe}", file=sys.stderr)
        return 1
    if not os.environ.get("DISPLAY"):
        print("DISPLAY is not set; run this script under xvfb-run", file=sys.stderr)
        return 1
    for tool in ("xdotool", "openbox"):
        if shutil.which(tool) is None:
            print(f"{tool} is required; install it or run the same script in CI", file=sys.stderr)
            return 1

    temp = Path(tempfile.mkdtemp(prefix="freeplots-gui-smoke-"))
    manager = subprocess.Popen(
        ["openbox"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
    )
    time.sleep(1.5)
    passed = 0
    failed = 0
    try:
        print("FreePlots Linux GUI smoke test")
        print(f"  executable: {exe}")
        print(f"  temp:       {temp}")
        cases = [
            ("default renderer", [], {}, True, False),
            ("wgpu", ["--renderer", "wgpu"], {}, False, True),
        ]
        for name, extra, env, resize, optional in cases:
            try:
                run_case(name, exe, extra, env, resize, optional, temp)
                passed += 1
            except Failure as error:
                print(f"  FAIL  {error}")
                failed += 1
    finally:
        manager.terminate()
        try:
            manager.wait(timeout=5)
        except subprocess.TimeoutExpired:
            manager.kill()
        if args.keep or failed == 0:
            shutil.rmtree(temp, ignore_errors=True)

    print("")
    print(f"smoke test: {passed} passed, {failed} failed")
    if failed:
        print(f"screenshots and logs kept in {temp}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
