# Third-party dependencies

FreePlots itself is MIT licensed. This file is generated from the locked
Cargo graph by `scripts/licence-inventory.py`; do not edit it by hand.
It lists every crate that can reach a shipped binary and separates the
always-linked runtime, the optional desktop window, build-time helpers and
development-only packages.

- Lockfile: `Cargo.lock` SHA-256 `a8c7ecdd056dd21fca1fc3ce0eff4a26012e0b95edc5daa1cec4ccf334341551`
- Build targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`
- Policy: `deny.toml`; CI runs `cargo deny check licenses advisories` and
  regenerates this file with `python3 scripts/licence-inventory.py --check`.
- Every dependency uses a permissive licence. Where a crate offers a choice
  that includes a copyleft branch, the right-hand column records the
  permissive branch FreePlots uses.

## Summary

| Role | Packages |
|---|---:|
| Runtime (always linked into the CLI) | 10 |
| Desktop (linked when the window feature is on) | 258 |
| Build time (proc-macros and helpers; never linked) | 34 |
| Development only (never shipped) | 20 |

## Runtime (always linked into the CLI)

| Crate | Version | Licence used | Declared | Platforms |
|---|---|---|---|---|
| `csv` | 1.4.0 | MIT | Unlicense OR MIT | all |
| `csv-core` | 0.1.13 | MIT | Unlicense OR MIT | all |
| `itoa` | 1.0.18 | MIT | MIT OR Apache-2.0 | all |
| `libm` | 0.2.15 | MIT |  | all |
| `memchr` | 2.8.3 | MIT | Unlicense OR MIT | all |
| `ryu` | 1.0.23 | Apache-2.0 | Apache-2.0 OR BSL-1.0 | all |
| `serde` | 1.0.228 | MIT | MIT OR Apache-2.0 | all |
| `serde_core` | 1.0.228 | MIT | MIT OR Apache-2.0 | all |
| `serde_json` | 1.0.145 | MIT | MIT OR Apache-2.0 | all |
| `thiserror` | 2.0.21 | MIT | MIT OR Apache-2.0 | all |

## Desktop (linked when the window feature is on)

| Crate | Version | Licence used | Declared | Platforms |
|---|---|---|---|---|
| `accesskit` | 0.24.1 | MIT | MIT OR Apache-2.0 | all |
| `accesskit_atspi_common` | 0.18.1 | MIT | MIT OR Apache-2.0 | Linux |
| `accesskit_consumer` | 0.35.0 | MIT | MIT OR Apache-2.0 | all |
| `accesskit_consumer` | 0.36.0 | MIT | MIT OR Apache-2.0 | Linux |
| `accesskit_unix` | 0.21.1 | MIT | MIT OR Apache-2.0 | Linux |
| `accesskit_windows` | 0.32.1 | MIT | MIT OR Apache-2.0 | Windows |
| `accesskit_winit` | 0.32.2 | Apache-2.0 |  | all |
| `adler2` | 2.0.1 | MIT | 0BSD OR MIT OR Apache-2.0 | all |
| `ahash` | 0.8.12 | MIT | MIT OR Apache-2.0 | all |
| `allocator-api2` | 0.2.21 | MIT | MIT OR Apache-2.0 | all |
| `arboard` | 3.7.0 | MIT | MIT OR Apache-2.0 | all |
| `arrayref` | 0.3.9 | BSD-2-Clause |  | all |
| `arrayvec` | 0.7.8 | MIT | MIT OR Apache-2.0 | all |
| `as-raw-xcb-connection` | 1.0.1 | MIT | MIT OR Apache-2.0 | Linux |
| `ash` | 0.38.0+1.3.281 | MIT | MIT OR Apache-2.0 | all |
| `async-broadcast` | 0.7.2 | MIT | MIT OR Apache-2.0 | Linux |
| `async-channel` | 2.5.0 | MIT | Apache-2.0 OR MIT | Linux |
| `async-executor` | 1.14.1 | MIT | Apache-2.0 OR MIT | Linux |
| `async-io` | 2.6.0 | MIT | Apache-2.0 OR MIT | Linux |
| `async-lock` | 3.4.2 | MIT | Apache-2.0 OR MIT | Linux |
| `async-process` | 2.5.0 | MIT | Apache-2.0 OR MIT | Linux |
| `async-signal` | 0.2.14 | MIT | Apache-2.0 OR MIT | Linux |
| `async-task` | 4.7.1 | MIT | Apache-2.0 OR MIT | Linux |
| `atomic-waker` | 1.1.2 | MIT | Apache-2.0 OR MIT | Linux |
| `atspi` | 0.29.0 | MIT | Apache-2.0 OR MIT | Linux |
| `atspi-common` | 0.13.0 | MIT | Apache-2.0 OR MIT | Linux |
| `atspi-proxies` | 0.13.0 | MIT | Apache-2.0 OR MIT | Linux |
| `base64` | 0.22.1 | MIT | MIT OR Apache-2.0 | all |
| `bit-set` | 0.10.0 | MIT | Apache-2.0 OR MIT | all |
| `bit-vec` | 0.9.1 | MIT | Apache-2.0 OR MIT | all |
| `bitflags` | 1.3.2 | MIT | MIT OR Apache-2.0 | all |
| `bitflags` | 2.13.2 | MIT | MIT OR Apache-2.0 | all |
| `blocking` | 1.7.0 | MIT | Apache-2.0 OR MIT | Linux |
| `bytemuck` | 1.25.2 | MIT | Zlib OR Apache-2.0 OR MIT | all |
| `byteorder-lite` | 0.1.0 | MIT | Unlicense OR MIT | all |
| `calloop` | 0.13.0 | MIT |  | Linux |
| `calloop` | 0.14.5 | MIT |  | Linux |
| `calloop-wayland-source` | 0.3.0 | MIT |  | Linux |
| `calloop-wayland-source` | 0.4.1 | MIT |  | Linux |
| `cfg-if` | 1.0.5 | MIT | MIT OR Apache-2.0 | all |
| `clipboard-win` | 5.4.3 | BSL-1.0 |  | Windows |
| `codespan-reporting` | 0.13.1 | Apache-2.0 |  | all |
| `color` | 0.3.3 | MIT | Apache-2.0 OR MIT | all |
| `color_quant` | 1.1.0 | MIT |  | all |
| `concurrent-queue` | 2.5.0 | MIT | Apache-2.0 OR MIT | Linux |
| `core_maths` | 0.1.1 | MIT |  | all |
| `crc32fast` | 1.5.2 | MIT | MIT OR Apache-2.0 | all |
| `crossbeam-utils` | 0.8.23 | MIT | MIT OR Apache-2.0 | Linux |
| `cursor-icon` | 1.2.0 | MIT | MIT OR Apache-2.0 OR Zlib | all |
| `data-url` | 0.3.2 | MIT | MIT OR Apache-2.0 | all |
| `dlib` | 0.5.3 | MIT |  | Linux |
| `downcast-rs` | 1.2.1 | MIT | MIT OR Apache-2.0 | Linux |
| `dpi` | 0.1.2 | Apache-2.0 AND MIT |  | all |
| `ecolor` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `eframe` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `egui` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `egui-wgpu` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `egui-winit` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `egui_extras` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `egui_glow` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `egui_kittest` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `either` | 1.19.0 | MIT | MIT OR Apache-2.0 | all |
| `emath` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `endi` | 1.1.1 | MIT |  | Linux |
| `enum-map` | 2.7.3 | MIT | MIT OR Apache-2.0 | all |
| `enumflags2` | 0.7.12 | MIT | MIT OR Apache-2.0 | Linux |
| `epaint` | 0.36.2 | MIT | MIT OR Apache-2.0 | all |
| `equivalent` | 1.0.3 | MIT | Apache-2.0 OR MIT | all |
| `errno` | 0.3.14 | MIT | MIT OR Apache-2.0 | Linux |
| `error-code` | 3.4.0 | BSL-1.0 |  | Windows |
| `euclid` | 0.22.14 | MIT | MIT OR Apache-2.0 | all |
| `event-listener` | 5.4.2 | MIT | Apache-2.0 OR MIT | Linux |
| `event-listener-strategy` | 0.5.4 | MIT | Apache-2.0 OR MIT | Linux |
| `fastrand` | 2.5.0 | MIT | Apache-2.0 OR MIT | Linux |
| `fdeflate` | 0.3.7 | MIT | MIT OR Apache-2.0 | all |
| `fearless_simd` | 0.4.1 | MIT | Apache-2.0 OR MIT | all |
| `flate2` | 1.1.10 | MIT | MIT OR Apache-2.0 | all |
| `float-cmp` | 0.9.0 | MIT |  | all |
| `foldhash` | 0.2.0 | Zlib |  | all |
| `font-types` | 0.12.7 | MIT | MIT OR Apache-2.0 | all |
| `fontconfig-parser` | 0.5.8 | MIT |  | Linux |
| `fontdb` | 0.23.0 | MIT |  | all |
| `futures-core` | 0.3.34 | MIT | MIT OR Apache-2.0 | Linux |
| `futures-io` | 0.3.34 | MIT | MIT OR Apache-2.0 | Linux |
| `futures-lite` | 2.6.1 | MIT | Apache-2.0 OR MIT | Linux |
| `futures-task` | 0.3.34 | MIT | MIT OR Apache-2.0 | Linux |
| `futures-util` | 0.3.34 | MIT | MIT OR Apache-2.0 | Linux |
| `gethostname` | 1.1.0 | Apache-2.0 |  | Linux |
| `getrandom` | 0.3.4 | MIT | MIT OR Apache-2.0 | Linux |
| `gif` | 0.13.3 | MIT | MIT OR Apache-2.0 | all |
| `glow` | 0.17.0 | MIT | MIT OR Apache-2.0 OR Zlib | all |
| `glutin` | 0.32.3 | Apache-2.0 |  | all |
| `glutin-winit` | 0.5.0 | MIT |  | all |
| `glutin_egl_sys` | 0.7.1 | Apache-2.0 |  | all |
| `glutin_glx_sys` | 0.6.1 | Apache-2.0 |  | Linux |
| `glutin_wgl_sys` | 0.6.1 | Apache-2.0 |  | Windows |
| `gpu-allocator` | 0.28.0 | MIT | MIT OR Apache-2.0 | all |
| `guillotiere` | 0.7.0 | MIT | MIT OR Apache-2.0 | all |
| `half` | 2.7.1 | MIT | MIT OR Apache-2.0 | all |
| `harfrust` | 0.12.0 | MIT |  | all |
| `hashbrown` | 0.16.1 | MIT | MIT OR Apache-2.0 | all |
| `hashbrown` | 0.17.1 | MIT | MIT OR Apache-2.0 | all |
| `hex` | 0.4.3 | MIT | MIT OR Apache-2.0 | Linux |
| `image` | 0.25.6 | MIT | MIT OR Apache-2.0 | all |
| `image-webp` | 0.2.4 | MIT | MIT OR Apache-2.0 | all |
| `imagesize` | 0.13.0 | MIT |  | all |
| `indexmap` | 2.14.2 | MIT | Apache-2.0 OR MIT | all |
| `itertools` | 0.15.0 | MIT | MIT OR Apache-2.0 | all |
| `jpeg-encoder` | 0.6.1 | MIT AND IJG | (MIT OR Apache-2.0) AND IJG | all |
| `khronos-egl` | 6.0.0 | MIT | MIT OR Apache-2.0 | all |
| `kittest` | 0.4.0 | MIT | MIT OR Apache-2.0 | all |
| `kurbo` | 0.11.3 | MIT | Apache-2.0 OR MIT | all |
| `kurbo` | 0.13.1 | MIT | Apache-2.0 OR MIT | all |
| `libc` | 0.2.191 | MIT | MIT OR Apache-2.0 | all |
| `libloading` | 0.8.9 | ISC |  | all |
| `linebender_resource_handle` | 0.1.1 | MIT | Apache-2.0 OR MIT | all |
| `linux-raw-sys` | 0.12.1 | MIT | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux |
| `linux-raw-sys` | 0.4.15 | MIT | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux |
| `lock_api` | 0.4.14 | MIT | MIT OR Apache-2.0 | all |
| `log` | 0.4.34 | MIT | MIT OR Apache-2.0 | all |
| `memmap2` | 0.9.11 | MIT | MIT OR Apache-2.0 | all |
| `miniz_oxide` | 0.8.9 | MIT | MIT OR Zlib OR Apache-2.0 | all |
| `miniz_oxide` | 0.9.1 | MIT | MIT OR Zlib OR Apache-2.0 | all |
| `naga` | 30.0.1 | MIT | MIT OR Apache-2.0 | all |
| `naga-types` | 30.0.1 | MIT | MIT OR Apache-2.0 | all |
| `nohash-hasher` | 0.2.0 | MIT | Apache-2.0 OR MIT | all |
| `num-traits` | 0.2.19 | MIT | MIT OR Apache-2.0 | all |
| `once_cell` | 1.21.4 | MIT | MIT OR Apache-2.0 | all |
| `ordered-float` | 5.5.1 | MIT |  | all |
| `ordered-stream` | 0.2.0 | MIT | MIT OR Apache-2.0 | Linux |
| `parking` | 2.2.1 | MIT | Apache-2.0 OR MIT | Linux |
| `parking_lot` | 0.12.5 | MIT | MIT OR Apache-2.0 | all |
| `parking_lot_core` | 0.9.12 | MIT | MIT OR Apache-2.0 | all |
| `peniko` | 0.6.1 | MIT | Apache-2.0 OR MIT | all |
| `percent-encoding` | 2.3.2 | MIT | MIT OR Apache-2.0 | Linux |
| `phf` | 0.13.1 | MIT |  | Linux |
| `phf_shared` | 0.13.1 | MIT |  | Linux |
| `pico-args` | 0.5.0 | MIT |  | all |
| `pin-project-lite` | 0.2.17 | MIT | Apache-2.0 OR MIT | all |
| `piper` | 0.2.5 | MIT | MIT OR Apache-2.0 | Linux |
| `png` | 0.17.16 | MIT | MIT OR Apache-2.0 | all |
| `polling` | 3.11.0 | MIT | Apache-2.0 OR MIT | Linux |
| `pollster` | 1.0.1 | MIT | Apache-2.0 OR MIT | all |
| `polycool` | 0.4.0 | MIT | MIT OR Apache-2.0 | all |
| `presser` | 0.3.1 | MIT | MIT OR Apache-2.0 | all |
| `proc-macro2` | 1.0.107 | MIT | MIT OR Apache-2.0 | all |
| `profiling` | 1.0.18 | MIT | MIT OR Apache-2.0 | all |
| `quick-error` | 2.0.1 | MIT | MIT OR Apache-2.0 | all |
| `quote` | 1.0.47 | MIT | MIT OR Apache-2.0 | all |
| `range-alloc` | 0.1.5 | MIT | MIT OR Apache-2.0 | Windows |
| `raw-window-handle` | 0.6.2 | MIT | MIT OR Apache-2.0 OR Zlib | all |
| `read-fonts` | 0.41.0 | MIT | MIT OR Apache-2.0 | all |
| `renderdoc-sys` | 1.1.0 | MIT | MIT OR Apache-2.0 | all |
| `resvg` | 0.45.1 | MIT | Apache-2.0 OR MIT | all |
| `rgb` | 0.8.53 | MIT |  | all |
| `roxmltree` | 0.20.0 | MIT | MIT OR Apache-2.0 | all |
| `rustc-hash` | 1.1.0 | MIT | Apache-2.0 OR MIT | all |
| `rustc-hash` | 2.1.3 | MIT | Apache-2.0 OR MIT | all |
| `rustix` | 0.38.44 | MIT | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux |
| `rustix` | 1.1.5 | MIT | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Linux |
| `rustybuzz` | 0.20.1 | MIT |  | all |
| `scoped-tls` | 1.0.1 | MIT | MIT OR Apache-2.0 | Linux |
| `scopeguard` | 1.2.0 | MIT | MIT OR Apache-2.0 | all |
| `self_cell` | 1.3.0 | Apache-2.0 | Apache-2.0 OR GPL-2.0-only | all |
| `serde_spanned` | 1.1.2 | MIT | MIT OR Apache-2.0 | all |
| `signal-hook-registry` | 1.4.8 | MIT | MIT OR Apache-2.0 | Linux |
| `simd-adler32` | 0.3.10 | MIT |  | all |
| `simplecss` | 0.2.2 | MIT | Apache-2.0 OR MIT | all |
| `siphasher` | 1.0.4 | MIT | MIT OR Apache-2.0 | all |
| `skrifa` | 0.44.0 | MIT | MIT OR Apache-2.0 | all |
| `slab` | 0.4.12 | MIT |  | Linux |
| `slotmap` | 1.1.1 | Zlib |  | all |
| `smallvec` | 1.16.3 | MIT | MIT OR Apache-2.0 | all |
| `smithay-client-toolkit` | 0.19.2 | MIT |  | Linux |
| `smithay-client-toolkit` | 0.20.0 | MIT |  | Linux |
| `smithay-clipboard` | 0.7.3 | MIT |  | Linux |
| `smol_str` | 0.2.2 | MIT | MIT OR Apache-2.0 | all |
| `spirv` | 0.4.0+sdk-1.4.341.0 | Apache-2.0 |  | all |
| `static_assertions` | 1.1.0 | MIT | MIT OR Apache-2.0 | all |
| `strict-num` | 0.1.1 | MIT |  | all |
| `svgtypes` | 0.15.3 | MIT | Apache-2.0 OR MIT | all |
| `syn` | 3.0.9 | MIT | MIT OR Apache-2.0 | all |
| `thiserror` | 1.0.69 | MIT | MIT OR Apache-2.0 | Linux |
| `tiny-skia` | 0.11.4 | BSD-3-Clause |  | all |
| `tiny-skia-path` | 0.11.4 | BSD-3-Clause |  | all |
| `tinyvec` | 1.13.3 | MIT | Zlib OR Apache-2.0 OR MIT | all |
| `toml` | 1.1.8+spec-1.1.0 | MIT | MIT OR Apache-2.0 | all |
| `toml_datetime` | 1.1.2+spec-1.1.0 | MIT | MIT OR Apache-2.0 | all |
| `toml_parser` | 1.1.5+spec-1.1.0 | MIT | MIT OR Apache-2.0 | all |
| `tracing` | 0.1.44 | MIT |  | all |
| `tracing-core` | 0.1.36 | MIT |  | all |
| `ttf-parser` | 0.25.1 | MIT | MIT OR Apache-2.0 | all |
| `type-map` | 0.5.1 | MIT | MIT OR Apache-2.0 | all |
| `unicode-bidi` | 0.3.18 | MIT | MIT OR Apache-2.0 | all |
| `unicode-bidi-mirroring` | 0.4.0 | MIT | MIT OR Apache-2.0 | all |
| `unicode-ccc` | 0.4.0 | MIT | MIT OR Apache-2.0 | all |
| `unicode-general-category` | 1.1.0 | Apache-2.0 |  | all |
| `unicode-ident` | 1.0.27 | MIT AND Unicode-3.0 | (MIT OR Apache-2.0) AND Unicode-3.0 | all |
| `unicode-properties` | 0.1.5 | MIT AND Unicode-3.0 | (MIT OR Apache-2.0) AND Unicode-3.0 | all |
| `unicode-script` | 0.5.9 | MIT | MIT OR Apache-2.0 | all |
| `unicode-segmentation` | 1.13.3 | MIT | MIT OR Apache-2.0 | all |
| `unicode-vo` | 0.1.0 | MIT | MIT OR Apache-2.0 | all |
| `unicode-width` | 0.2.2 | MIT | MIT OR Apache-2.0 | all |
| `usvg` | 0.45.1 | MIT | Apache-2.0 OR MIT | all |
| `uuid` | 1.28.0 | MIT | Apache-2.0 OR MIT | all |
| `vello_common` | 0.1.0 | MIT | Apache-2.0 OR MIT | all |
| `vello_cpu` | 0.1.0 | MIT | Apache-2.0 OR MIT | all |
| `wayland-backend` | 0.3.17 | MIT |  | Linux |
| `wayland-client` | 0.31.15 | MIT |  | Linux |
| `wayland-csd-frame` | 0.3.0 | MIT |  | Linux |
| `wayland-cursor` | 0.31.14 | MIT |  | Linux |
| `wayland-protocols` | 0.32.13 | MIT |  | Linux |
| `wayland-protocols-experimental` | 20250721.0.1 | MIT |  | Linux |
| `wayland-protocols-misc` | 0.3.12 | MIT |  | Linux |
| `wayland-protocols-plasma` | 0.3.12 | MIT |  | Linux |
| `wayland-protocols-wlr` | 0.3.12 | MIT |  | Linux |
| `wayland-sys` | 0.31.11 | MIT |  | Linux |
| `web-time` | 1.1.0 | MIT | MIT OR Apache-2.0 | all |
| `weezl` | 0.1.12 | MIT | MIT OR Apache-2.0 | all |
| `wgpu` | 30.0.1 | MIT | MIT OR Apache-2.0 | all |
| `wgpu-core` | 30.0.1 | MIT | MIT OR Apache-2.0 | all |
| `wgpu-core-deps-windows-linux-android` | 30.0.1 | MIT | MIT OR Apache-2.0 | all |
| `wgpu-hal` | 30.0.1 | MIT | MIT OR Apache-2.0 | all |
| `wgpu-naga-bridge` | 30.0.1 | MIT | MIT OR Apache-2.0 | all |
| `wgpu-types` | 30.0.1 | MIT | MIT OR Apache-2.0 | all |
| `windows` | 0.62.2 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-collections` | 0.3.2 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-core` | 0.62.2 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-future` | 0.3.2 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-link` | 0.2.1 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-numerics` | 0.3.1 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-result` | 0.4.1 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-strings` | 0.5.1 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-sys` | 0.52.0 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-sys` | 0.61.2 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-targets` | 0.52.6 | MIT | MIT OR Apache-2.0 | Windows |
| `windows-threading` | 0.2.1 | MIT | MIT OR Apache-2.0 | Windows |
| `windows_aarch64_msvc` | 0.52.6 | MIT | MIT OR Apache-2.0 | Windows |
| `windows_x86_64_msvc` | 0.52.6 | MIT | MIT OR Apache-2.0 | Windows |
| `winit` | 0.30.13 | Apache-2.0 |  | all |
| `winnow` | 1.0.4 | MIT |  | all |
| `x11-dl` | 2.21.0 | MIT |  | Linux |
| `x11rb` | 0.13.2 | MIT | MIT OR Apache-2.0 | Linux |
| `x11rb-protocol` | 0.13.2 | MIT | MIT OR Apache-2.0 | Linux |
| `xcursor` | 0.3.11 | MIT |  | Linux |
| `xkbcommon-dl` | 0.4.2 | MIT |  | Linux |
| `xkeysym` | 0.2.1 | MIT | MIT OR Apache-2.0 OR Zlib | Linux |
| `xmlwriter` | 0.1.0 | MIT |  | all |
| `zbus` | 5.19.0 | MIT |  | Linux |
| `zbus-lockstep` | 0.5.2 | MIT |  | Linux |
| `zbus_names` | 4.3.5 | MIT |  | Linux |
| `zbus_xml` | 5.2.1 | MIT |  | Linux |
| `zcheapstr` | 1.2.0 | MIT |  | Linux |
| `zerocopy` | 0.8.63 | MIT | BSD-2-Clause OR Apache-2.0 OR MIT | all |
| `zune-core` | 0.4.12 | MIT | MIT OR Apache-2.0 OR Zlib | all |
| `zune-jpeg` | 0.4.21 | MIT | MIT OR Apache-2.0 OR Zlib | all |
| `zvariant` | 5.15.1 | MIT |  | Linux |
| `zvariant_utils` | 4.2.0 | MIT |  | Linux |

## Build time (proc-macros and helpers; never linked into a shipped binary)

| Crate | Version | Licence used | Declared | Platforms |
|---|---|---|---|---|
| `async-recursion` | 1.2.0 | MIT | MIT OR Apache-2.0 | all |
| `async-trait` | 0.1.92 | MIT | MIT OR Apache-2.0 | all |
| `autocfg` | 1.5.1 | MIT | Apache-2.0 OR MIT | host |
| `bytemuck_derive` | 1.12.1 | MIT | Zlib OR Apache-2.0 OR MIT | all |
| `document-features` | 0.2.12 | MIT | MIT OR Apache-2.0 | all |
| `enum-map-derive` | 0.17.0 | MIT | MIT OR Apache-2.0 | all |
| `enumflags2_derive` | 0.7.12 | MIT | MIT OR Apache-2.0 | all |
| `futures-macro` | 0.3.34 | MIT | MIT OR Apache-2.0 | all |
| `js-sys` | 0.3.106 | MIT | MIT OR Apache-2.0 | host |
| `litrs` | 1.0.1 | MIT | MIT OR Apache-2.0 | all |
| `phf_generator` | 0.13.1 | MIT |  | all |
| `phf_macros` | 0.13.1 | MIT |  | all |
| `proc-macro-crate` | 3.5.0 | MIT | MIT OR Apache-2.0 | all |
| `quick-xml` | 0.41.0 | MIT |  | all |
| `redox_syscall` | 0.5.18 | MIT |  | host |
| `serde_derive` | 1.0.228 | MIT | MIT OR Apache-2.0 | all |
| `serde_repr` | 0.1.21 | MIT | MIT OR Apache-2.0 | all |
| `syn` | 2.0.119 | MIT | MIT OR Apache-2.0 | all |
| `thiserror-impl` | 1.0.69 | MIT | MIT OR Apache-2.0 | all |
| `thiserror-impl` | 2.0.21 | MIT | MIT OR Apache-2.0 | all |
| `toml_edit` | 0.25.17+spec-1.1.0 | MIT | MIT OR Apache-2.0 | all |
| `tracing-attributes` | 0.1.31 | MIT |  | all |
| `version_check` | 0.9.5 | MIT | MIT OR Apache-2.0 | host |
| `wasm-bindgen` | 0.2.129 | MIT | MIT OR Apache-2.0 | host |
| `wasm-bindgen-shared` | 0.2.129 | MIT | MIT OR Apache-2.0 | host |
| `wayland-scanner` | 0.31.11 | MIT |  | all |
| `web-sys` | 0.3.106 | MIT | MIT OR Apache-2.0 | host |
| `windows-implement` | 0.60.2 | MIT | MIT OR Apache-2.0 | all |
| `windows-interface` | 0.59.3 | MIT | MIT OR Apache-2.0 | all |
| `winresource` | 0.1.34 | MIT |  | host |
| `zbus-lockstep-macros` | 0.5.2 | MIT |  | all |
| `zbus_macros` | 5.19.0 | MIT |  | all |
| `zerocopy-derive` | 0.8.63 | MIT | BSD-2-Clause OR Apache-2.0 OR MIT | all |
| `zvariant_derive` | 5.15.1 | MIT |  | all |

## Development only (never shipped)

| Crate | Version | Licence used | Declared | Platforms |
|---|---|---|---|---|
| `bit-set` | 0.8.0 | MIT | Apache-2.0 OR MIT | host |
| `bit-vec` | 0.8.0 | MIT | Apache-2.0 OR MIT | host |
| `fnv` | 1.0.7 | MIT | Apache-2.0 OR MIT | host |
| `getrandom` | 0.4.3 | MIT | MIT OR Apache-2.0 | host |
| `ppv-lite86` | 0.2.21 | MIT | MIT OR Apache-2.0 | host |
| `proptest` | 1.9.0 | MIT | MIT OR Apache-2.0 | host |
| `quick-error` | 1.2.3 | MIT | MIT OR Apache-2.0 | host |
| `r-efi` | 5.3.0 | MIT | MIT OR Apache-2.0 OR LGPL-2.1-or-later | host |
| `r-efi` | 6.0.0 | MIT | MIT OR Apache-2.0 OR LGPL-2.1-or-later | host |
| `rand` | 0.9.5 | MIT | MIT OR Apache-2.0 | host |
| `rand_chacha` | 0.9.0 | MIT | MIT OR Apache-2.0 | host |
| `rand_core` | 0.9.5 | MIT | MIT OR Apache-2.0 | host |
| `rand_xorshift` | 0.4.0 | MIT | MIT OR Apache-2.0 | host |
| `regex-syntax` | 0.8.11 | MIT | MIT OR Apache-2.0 | host |
| `rusty-fork` | 0.3.1 | MIT | MIT OR Apache-2.0 | host |
| `tempfile` | 3.27.0 | MIT | MIT OR Apache-2.0 | host |
| `unarray` | 0.1.4 | MIT | MIT OR Apache-2.0 | host |
| `wait-timeout` | 0.2.1 | MIT | MIT OR Apache-2.0 | host |
| `wasip2` | 1.0.4+wasi-0.2.12 | MIT | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | host |
| `wit-bindgen` | 0.57.1 | MIT | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | host |

Per-package authors and repositories are in `licenses/DEPENDENCIES.csv`;
the machine-readable inventory is `licenses/sbom.spdx.json`.

## Licence texts

### MIT License

```text
MIT License

Copyright (c) the copyright holders listed for each package in
licenses/DEPENDENCIES.csv

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### Apache License 2.0

```text
Apache License
                        Version 2.0, January 2004
                     http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.

   "License" shall mean the terms and conditions for use, reproduction,
   and distribution as defined by Sections 1 through 9 of this document.

   "Licensor" shall mean the copyright owner or entity authorized by
   the copyright owner that is granting the License.

   "Legal Entity" shall mean the union of the acting entity and all
   other entities that control, are controlled by, or are under common
   control with that entity. For the purposes of this definition,
   "control" means (i) the power, direct or indirect, to cause the
   direction or management of such entity, whether by contract or
   otherwise, or (ii) ownership of fifty percent (50%) or more of the
   outstanding shares, or (iii) beneficial ownership of such entity.

   "You" (or "Your") shall mean an individual or Legal Entity
   exercising permissions granted by this License.

   "Source" form shall mean the preferred form for making modifications,
   including but not limited to software source code, documentation
   source, and configuration files.

   "Object" form shall mean any form resulting from mechanical
   transformation or translation of a Source form, including but
   not limited to compiled object code, generated documentation,
   and conversions to other media types.

   "Work" shall mean the work of authorship, whether in Source or
   Object form, made available under the License, as indicated by a
   copyright notice that is included in or attached to the work
   (an example is provided in the Appendix below).

   "Derivative Works" shall mean any work, whether in Source or Object
   form, that is based on (or derived from) the Work and for which the
   editorial revisions, annotations, elaborations, or other modifications
   represent, as a whole, an original work of authorship. For the purposes
   of this License, Derivative Works shall not include works that remain
   separable from, or merely link (or bind by name) to the interfaces of,
   the Work and Derivative Works thereof.

   "Contribution" shall mean any work of authorship, including
   the original version of the Work and any modifications or additions
   to that Work or Derivative Works thereof, that is intentionally
   submitted to Licensor for inclusion in the Work by the copyright owner
   or by an individual or Legal Entity authorized to submit on behalf of
   the copyright owner. For the purposes of this definition, "submitted"
   means any form of electronic, verbal, or written communication sent
   to the Licensor or its representatives, including but not limited to
   communication on electronic mailing lists, source code control systems,
   and issue tracking systems that are managed by, or on behalf of, the
   Licensor for the purpose of discussing and improving the Work, but
   excluding communication that is conspicuously marked or otherwise
   designated in writing by the copyright owner as "Not a Contribution."

   "Contributor" shall mean Licensor and any individual or Legal Entity
   on behalf of whom a Contribution has been received by Licensor and
   subsequently incorporated within the Work.

2. Grant of Copyright License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   copyright license to reproduce, prepare Derivative Works of,
   publicly display, publicly perform, sublicense, and distribute the
   Work and such Derivative Works in Source or Object form.

3. Grant of Patent License. Subject to the terms and conditions of
   this License, each Contributor hereby grants to You a perpetual,
   worldwide, non-exclusive, no-charge, royalty-free, irrevocable
   (except as stated in this section) patent license to make, have made,
   use, offer to sell, sell, import, and otherwise transfer the Work,
   where such license applies only to those patent claims licensable
   by such Contributor that are necessarily infringed by their
   Contribution(s) alone or by combination of their Contribution(s)
   with the Work to which such Contribution(s) was submitted. If You
   institute patent litigation against any entity (including a
   cross-claim or counterclaim in a lawsuit) alleging that the Work
   or a Contribution incorporated within the Work constitutes direct
   or contributory patent infringement, then any patent licenses
   granted to You under this License for that Work shall terminate
   as of the date such litigation is filed.

4. Redistribution. You may reproduce and distribute copies of the
   Work or Derivative Works thereof in any medium, with or without
   modifications, and in Source or Object form, provided that You
   meet the following conditions:

   (a) You must give any other recipients of the Work or
       Derivative Works a copy of this License; and

   (b) You must cause any modified files to carry prominent notices
       stating that You changed the files; and

   (c) You must retain, in the Source form of any Derivative Works
       that You distribute, all copyright, patent, trademark, and
       attribution notices from the Source form of the Work,
       excluding those notices that do not pertain to any part of
       the Derivative Works; and

   (d) If the Work includes a "NOTICE" text file as part of its
       distribution, then any Derivative Works that You distribute must
       include a readable copy of the attribution notices contained
       within such NOTICE file, excluding those notices that do not
       pertain to any part of the Derivative Works, in at least one
       of the following places: within a NOTICE text file distributed
       as part of the Derivative Works; within the Source form or
       documentation, if provided along with the Derivative Works; or,
       within a display generated by the Derivative Works, if and
       wherever such third-party notices normally appear. The contents
       of the NOTICE file are for informational purposes only and
       do not modify the License. You may add Your own attribution
       notices within Derivative Works that You distribute, alongside
       or as an addendum to the NOTICE text from the Work, provided
       that such additional attribution notices cannot be construed
       as modifying the License.

   You may add Your own copyright statement to Your modifications and
   may provide additional or different license terms and conditions
   for use, reproduction, or distribution of Your modifications, or
   for any such Derivative Works as a whole, provided Your use,
   reproduction, and distribution of the Work otherwise complies with
   the conditions stated in this License.

5. Submission of Contributions. Unless You explicitly state otherwise,
   any Contribution intentionally submitted for inclusion in the Work
   by You to the Licensor shall be under the terms and conditions of
   this License, without any additional terms or conditions.
   Notwithstanding the above, nothing herein shall supersede or modify
   the terms of any separate license agreement you may have executed
   with Licensor regarding such Contributions.

6. Trademarks. This License does not grant permission to use the trade
   names, trademarks, service marks, or product names of the Licensor,
   except as required for reasonable and customary use in describing the
   origin of the Work and reproducing the content of the NOTICE file.

7. Disclaimer of Warranty. Unless required by applicable law or
   agreed to in writing, Licensor provides the Work (and each
   Contributor provides its Contributions) on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
   implied, including, without limitation, any warranties or conditions
   of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
   PARTICULAR PURPOSE. You are solely responsible for determining the
   appropriateness of using or redistributing the Work and assume any
   risks associated with Your exercise of permissions under this License.

8. Limitation of Liability. In no event and under no legal theory,
   whether in tort (including negligence), contract, or otherwise,
   unless required by applicable law (such as deliberate and grossly
   negligent acts) or agreed to in writing, shall any Contributor be
   liable to You for damages, including any direct, indirect, special,
   incidental, or consequential damages of any character arising as a
   result of this License or out of the use or inability to use the
   Work (including but not limited to damages for loss of goodwill,
   work stoppage, computer failure or malfunction, or any and all
   other commercial damages or losses), even if such Contributor
   has been advised of the possibility of such damages.

9. Accepting Warranty or Additional Liability. While redistributing
   the Work or Derivative Works thereof, You may choose to offer,
   and charge a fee for, acceptance of support, warranty, indemnity,
   or other liability obligations and/or rights consistent with this
   License. However, in accepting such obligations, You may act only
   on Your own behalf and on Your sole responsibility, not on behalf
   of any other Contributor, and only if You agree to indemnify,
   defend, and hold each Contributor harmless for any liability
   incurred by, or claims asserted against, such Contributor by reason
   of your accepting any such warranty or additional liability.

END OF TERMS AND CONDITIONS
```

### BSD 3-Clause License

```text
BSD 3-Clause License

Copyright (c) the copyright holders listed for each package in
licenses/DEPENDENCIES.csv
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its contributors
   may be used to endorse or promote products derived from this software
   without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### BSD 2-Clause License

```text
BSD 2-Clause License

Copyright (c) the copyright holders listed for each package in
licenses/DEPENDENCIES.csv
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice,
   this list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

### ISC License

```text
ISC License

Copyright (c) the copyright holders listed for each package in
licenses/DEPENDENCIES.csv

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
PERFORMANCE OF THIS SOFTWARE.
```

### zlib License

```text
zlib License

Copyright (c) the copyright holders listed for each package in
licenses/DEPENDENCIES.csv

This software is provided 'as-is', without any express or implied warranty.
In no event will the authors be held liable for any damages arising from the
use of this software.

Permission is granted to anyone to use this software for any purpose,
including commercial applications, and to alter it and redistribute it
freely, subject to the following restrictions:

1. The origin of this software must not be misrepresented; you must not claim
   that you wrote the original software. If you use this software in a
   product, an acknowledgment in the product documentation would be
   appreciated but is not required.

2. Altered source versions must be plainly marked as such, and must not be
   misrepresented as being the original software.

3. This notice may not be removed or altered from any source distribution.
```

### Boost Software License 1.0

```text
Boost Software License - Version 1.0 - August 17th, 2003

Permission is hereby granted, free of charge, to any person or organization
obtaining a copy of the software and accompanying documentation covered by
this license (the "Software") to use, reproduce, display, distribute,
execute, and transmit the Software, and to prepare derivative works of the
Software, and to permit third-parties to whom the Software is furnished to
do so, all subject to the following:

The copyright notices in the Software and this entire statement, including
the above license grant, this restriction and the following disclaimer,
must be included in all copies of the Software, in whole or in part, and
all derivative works of the Software, unless such copies or derivative
works are solely in the form of machine-executable object code generated by
a source language processor.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE, TITLE AND NON-INFRINGEMENT. IN NO EVENT
SHALL THE COPYRIGHT HOLDERS OR ANYONE DISTRIBUTING THE SOFTWARE BE LIABLE
FOR ANY DAMAGES OR OTHER LIABILITY, WHETHER IN CONTRACT, TORT OR OTHERWISE,
ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

### Independent JPEG Group licence

```text
Independent JPEG Group (IJG) licence

The authors make NO WARRANTY or representation, either express or implied,
with respect to this software, its quality, accuracy, merchantability, or
fitness for a particular purpose. This software is provided "AS IS", and you,
its user, assume the entire risk as to its quality and accuracy.

This software is copyright (C) 1991-1998, Thomas G. Lane.
All Rights Reserved except as specified below.

Permission is hereby granted to use, copy, modify, and distribute this software
(or portions thereof) for any purpose, without fee, subject to these
conditions:

(1) If any part of the source code for this software is distributed, then this
README file must be included, with this copyright and no-warranty notice
unmodified; and any recipient of this software must be informed that the
software is available from the same place as the original.

(2) If only executable code is distributed, then the accompanying
documentation must state that "this software is based in part on the work of
the Independent JPEG Group".

(3) Permission for use of this software is granted only if the user accepts
full responsibility for any undesirable consequences; the authors accept NO
LIABILITY for damages of any kind.

These conditions apply to any software derived from or based on the IJG code,
not just to the unmodified library. If you use our work, you ought to
acknowledge us.

Permission is NOT granted for the use of any IJG author's name or company name
in advertising or publicity relating to this software or products derived from
it. This software may be referred to only as "the Independent JPEG Group's
software".

We specifically permit and encourage the use of this software as the basis of
commercial products, provided that all warranty or liability claims are
assumed by the product vendor.
```

### Unicode License V3

```text
UNICODE LICENSE V3

COPYRIGHT AND PERMISSION NOTICE

Copyright © 1991-2023 Unicode, Inc.

NOTICE TO USER: Carefully read the following legal agreement. BY
DOWNLOADING, INSTALLING, COPYING OR OTHERWISE USING DATA FILES, AND/OR
SOFTWARE, YOU UNEQUIVOCALLY ACCEPT, AND AGREE TO BE BOUND BY, ALL OF THE
TERMS AND CONDITIONS OF THIS AGREEMENT. IF YOU DO NOT AGREE, DO NOT
DOWNLOAD, INSTALL, COPY, DISTRIBUTE OR USE THE DATA FILES OR SOFTWARE.

Permission is hereby granted, free of charge, to any person obtaining a
copy of data files and any associated documentation (the "Data Files") or
software and any associated documentation (the "Software") to deal in the
Data Files or Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, and/or sell
copies of the Data Files or Software, and to permit persons to whom the
Data Files or Software are furnished to do so, provided that either (a)
this copyright and permission notice appear with all copies of the Data
Files or Software, or (b) this copyright and permission notice appear in
associated Documentation.

THE DATA FILES AND SOFTWARE ARE PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
THIRD PARTY RIGHTS.

IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS INCLUDED IN THIS NOTICE
BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT OR CONSEQUENTIAL DAMAGES,
OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THE DATA
FILES OR SOFTWARE.

Except as contained in this notice, the name of a copyright holder shall
not be used in advertising or otherwise to promote the sale, use or other
dealings in these Data Files or Software without prior written
authorization of the copyright holder.
```
