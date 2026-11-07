# Starts the desktop application on a Windows desktop, asks its automation bridge
# for a screenshot and checks that the window really painted: a renderer that
# opens a blank (all-white or all-black) client area fails here.
#
# The runner has no GPU, so wgpu draws through D3D12 WARP. glow is optional: it
# either paints a frame or exits with a clear error; it must not hang or stay
# blank. The same script runs on a normal Windows machine:
#
#   powershell -NoProfile -File scripts/windows-gui-smoke.ps1 -Exe C:\path\FreePlots.exe
#
# Exit code 0 means every case passed; 1 means at least one failed.

param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [int]$StartPort = 47830,
    [int]$StartupTimeoutSec = 180
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

try {
    Add-Type -AssemblyName System.Drawing -ErrorAction Stop
} catch {
    throw "System.Drawing is required to inspect the screenshots: $($_.Exception.Message)"
}

Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class FreePlotsWindow
{
    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr hWnd, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder text, int count);

    public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumProc callback, IntPtr lParam);

    private static IntPtr found = IntPtr.Zero;
    private static string wanted = "FreePlots";

    public static IntPtr MainWindow(uint pid)
    {
        found = IntPtr.Zero;
        EnumWindows(delegate(IntPtr hWnd, IntPtr lParam)
        {
            uint owner;
            GetWindowThreadProcessId(hWnd, out owner);
            if (owner != pid || !IsWindowVisible(hWnd)) return true;
            StringBuilder text = new StringBuilder(256);
            GetWindowTextW(hWnd, text, text.Capacity);
            // The app titles the window "FreePlots - <source>"; a graph window
            // has its own title and must not be mistaken for it.
            if (text.ToString().StartsWith(wanted)) { found = hWnd; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@

$script:passed = 0
$script:failed = 0

function Write-Pass([string]$Message) {
    Write-Host "  PASS  $Message"
    $script:passed++
}

function Write-Fail([string]$Message) {
    Write-Host "  FAIL  $Message"
    $script:failed++
}

function Start-App {
    param(
        [string]$ExePath,
        [int]$Port,
        [string[]]$ExtraArgs,
        [hashtable]$Environment
    )
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = (Resolve-Path -LiteralPath $ExePath).Path
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $false
    foreach ($arg in (@('--gui', '--bridge', "127.0.0.1:$Port") + $ExtraArgs)) {
        [void]$psi.ArgumentList.Add($arg)
    }
    # A variable set by the runner or by a previous case must not decide this
    # case's backend.
    [void]$psi.Environment.Remove('FREEPLOTS_RENDERER')
    foreach ($key in $Environment.Keys) {
        $psi.Environment[$key] = $Environment[$key]
    }
    $process = [System.Diagnostics.Process]::Start($psi)
    [pscustomobject]@{
        Process = $process
        Stdout = $process.StandardOutput.ReadToEndAsync()
        Stderr = $process.StandardError.ReadToEndAsync()
    }
}

function Get-AppLog($App) {
    $out = ''
    $err = ''
    # Do not block on a process that is still running: give it two seconds to
    # flush and take whatever arrived.
    try { if ($App.Stdout.Wait(2000)) { $out = $App.Stdout.GetAwaiter().GetResult() } } catch { }
    try { if ($App.Stderr.Wait(2000)) { $err = $App.Stderr.GetAwaiter().GetResult() } } catch { }
    (($out, $err) | Where-Object { $_ -and $_.Trim().Length -gt 0 }) -join "`n"
}

function Stop-App($App, [int]$TimeoutSec = 20) {
    $process = $App.Process
    if ($process.HasExited) { return $process.ExitCode }
    if (-not $process.CloseMainWindow()) {
        if ($process.HasExited) { return $process.ExitCode }
        $process.Kill()
        [void]$process.WaitForExit(5000)
        throw "the application has no main window to close"
    }
    if (-not $process.WaitForExit($TimeoutSec * 1000)) {
        $process.Kill()
        [void]$process.WaitForExit(5000)
        throw "the application did not close within $TimeoutSec s of WM_CLOSE"
    }
    return $process.ExitCode
}

function New-Bridge {
    param($App, [int]$Port, [int]$TimeoutSec)
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if ($App.Process.HasExited) { return $null }
        $client = New-Object System.Net.Sockets.TcpClient
        try {
            $task = $client.ConnectAsync('127.0.0.1', $Port)
            if ($task.Wait(1000) -and $client.Connected) {
                $client.ReceiveTimeout = 60000
                $client.NoDelay = $true
                $stream = $client.GetStream()
                return [pscustomobject]@{
                    Client = $client
                    Reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8, $false, 4096, $true)
                    Writer = New-Object System.IO.StreamWriter($stream, (New-Object System.Text.UTF8Encoding($false)), 4096, $true)
                }
            }
        } catch {
        }
        $client.Dispose()
        Start-Sleep -Milliseconds 500
    }
    return $null
}

function Invoke-Bridge($Bridge, [hashtable]$Request) {
    $line = ($Request | ConvertTo-Json -Compress)
    $Bridge.Writer.WriteLine($line)
    $Bridge.Writer.Flush()
    $reply = $Bridge.Reader.ReadLine()
    if ($null -eq $reply -or $reply.Length -eq 0) {
        throw "the bridge closed the connection"
    }
    return ($reply | ConvertFrom-Json)
}

function Save-Screenshot($Bridge, [string]$Path) {
    Remove-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
    $reply = Invoke-Bridge $Bridge @{ cmd = 'screenshot'; path = $Path }
    if (-not $reply.ok) {
        throw "screenshot command failed: $($reply.error)"
    }
    if (-not (Test-Path -LiteralPath $Path)) {
        throw "the screenshot was not written to $Path"
    }
    return $reply
}

function Measure-Shot([string]$Path) {
    $bitmap = [System.Drawing.Bitmap]::FromFile((Resolve-Path -LiteralPath $Path).Path)
    try {
        $counts = New-Object 'System.Collections.Generic.Dictionary[int,int]'
        $ink = 0
        $samples = 0
        $stepX = [Math]::Max(1, [int]($bitmap.Width / 180))
        $stepY = [Math]::Max(1, [int]($bitmap.Height / 120))
        for ($y = 0; $y -lt $bitmap.Height; $y += $stepY) {
            for ($x = 0; $x -lt $bitmap.Width; $x += $stepX) {
                $colour = $bitmap.GetPixel($x, $y)
                $argb = $colour.ToArgb()
                if ($counts.ContainsKey($argb)) { $counts[$argb]++ } else { $counts[$argb] = 1 }
                $luminance = 0.299 * $colour.R + 0.587 * $colour.G + 0.114 * $colour.B
                if ($luminance -lt 240) { $ink++ }
                $samples++
            }
        }
        $top = 0
        foreach ($value in $counts.Values) {
            if ($value -gt $top) { $top = $value }
        }
        [pscustomobject]@{
            Width = $bitmap.Width
            Height = $bitmap.Height
            Colours = $counts.Count
            TopShare = $top / [double]$samples
            Ink = $ink / [double]$samples
            Bytes = (Get-Item -LiteralPath $Path).Length
        }
    } finally {
        $bitmap.Dispose()
    }
}

function Assert-Rendered($Shot, [string]$Label) {
    $ink = [Math]::Round($Shot.Ink * 100, 1)
    $top = [Math]::Round($Shot.TopShare * 100, 1)
    # The bridge writes raw RGBA rows in stored deflate blocks, so a full frame
    # is about four bytes per pixel. Half of that is already generous.
    if ($Shot.Bytes -lt ($Shot.Width * $Shot.Height * 2)) {
        throw "$Label`: the screenshot is only $($Shot.Bytes) bytes for $($Shot.Width)x$($Shot.Height)"
    }
    if ($Shot.Colours -lt 16) {
        throw "$Label`: only $($Shot.Colours) distinct colours (the window is blank)"
    }
    if ($Shot.TopShare -gt 0.995) {
        throw "$Label`: one colour covers $top% of the window (the window is blank)"
    }
    if ($Shot.Ink -lt 0.005) {
        throw "$Label`: only $ink% of the window is drawn (the window is blank)"
    }
    if ($Shot.Ink -gt 0.98) {
        throw "$Label`: $ink% of the window is drawn (the window is a solid block)"
    }
    Write-Pass "$Label painted $($Shot.Width)x$($Shot.Height), $($Shot.Colours) colours, $ink% ink, $($Shot.Bytes) bytes"
}

function Close-Cleanly($App, [string]$Label) {
    $code = Stop-App $App
    if ($code -ne 0) {
        throw "$Label`: exit code $code`n$(Get-AppLog $App)"
    }
    Write-Pass "$Label closed cleanly (exit 0)"
}

function Invoke-WindowCase {
    param(
        [string]$Name,
        [string]$ExePath,
        [int]$Port,
        [string[]]$ExtraArgs,
        [hashtable]$Environment,
        [switch]$Resize
    )
    Write-Host "=== $Name"
    $app = Start-App -ExePath $ExePath -Port $Port -ExtraArgs $ExtraArgs -Environment $Environment
    try {
        $bridge = New-Bridge -App $app -Port $Port -TimeoutSec $StartupTimeoutSec
        if ($null -eq $bridge) {
            throw "$Name`: no bridge on 127.0.0.1:$Port within $StartupTimeoutSec s`n$(Get-AppLog $app)"
        }
        try {
            $first = Save-Screenshot -Bridge $bridge -Path (Join-Path $env:RUNNER_TEMP "$Name.png")
            Assert-Rendered (Measure-Shot $first.path) $Name

            if ($Resize) {
                $window = [FreePlotsWindow]::MainWindow([uint32]$app.Process.Id)
                if ($window -eq [IntPtr]::Zero) {
                    throw "$Name`: no visible window titled FreePlots"
                }
                # SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE
                $flags = 0x0002 -bor 0x0004 -bor 0x0010
                if (-not [FreePlotsWindow]::SetWindowPos($window, [IntPtr]::Zero, 0, 0, 900, 640, $flags)) {
                    throw "$Name`: SetWindowPos failed"
                }
                Start-Sleep -Seconds 2
                $second = Save-Screenshot -Bridge $bridge -Path (Join-Path $env:RUNNER_TEMP "$Name-resized.png")
                Assert-Rendered (Measure-Shot $second.path) "$Name (resized)"
                if ([Math]::Abs([int]$second.width - 900) -gt 60) {
                    throw "$Name`: the window reports $($second.width) px wide after a resize to 900 px"
                }
                Write-Pass "$Name resized to $($second.width)x$($second.height)"
            }
        } finally {
            $bridge.Client.Dispose()
        }
        Close-Cleanly $app $Name
    } finally {
        if (-not $app.Process.HasExited) {
            $app.Process.Kill()
            [void]$app.Process.WaitForExit(5000)
        }
    }
}

function Invoke-OptionalGlowCase {
    param([string]$ExePath, [int]$Port)
    Write-Host "=== glow (optional on a runner without OpenGL)"
    $app = Start-App -ExePath $ExePath -Port $Port -ExtraArgs @('--renderer', 'glow') -Environment @{}
    try {
        $bridge = New-Bridge -App $app -Port $Port -TimeoutSec 90
        if ($null -eq $bridge) {
            if (-not $app.Process.HasExited) {
                throw "glow: the application neither painted nor exited within 90 s"
            }
            $code = $app.Process.ExitCode
            $log = Get-AppLog $app
            if ($code -ne 0 -and $log.Trim().Length -gt 0) {
                Write-Pass "glow is unavailable here (exit $code): $($log.Trim())"
                return
            }
            throw "glow: exit code $code without an explanation`n$log"
        }
        try {
            $shot = Save-Screenshot -Bridge $bridge -Path (Join-Path $env:RUNNER_TEMP 'glow.png')
            Assert-Rendered (Measure-Shot $shot.path) 'glow'
        } finally {
            $bridge.Client.Dispose()
        }
        Close-Cleanly $app 'glow'
    } finally {
        if (-not $app.Process.HasExited) {
            $app.Process.Kill()
            [void]$app.Process.WaitForExit(5000)
        }
    }
}

function Invoke-InvalidRendererCase {
    param([string]$ExePath)
    Write-Host "=== invalid renderer"
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = (Resolve-Path -LiteralPath $ExePath).Path
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    [void]$psi.ArgumentList.Add('--renderer')
    [void]$psi.ArgumentList.Add('banana')
    $process = [System.Diagnostics.Process]::Start($psi)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(30000)) {
        $process.Kill()
        throw "invalid renderer: the application did not exit"
    }
    $text = ''
    try { $text = ($stdout.GetAwaiter().GetResult() + "`n" + $stderr.GetAwaiter().GetResult()) } catch { }
    if ($process.ExitCode -ne 1) {
        throw "invalid renderer: exit code $($process.ExitCode), expected 1`n$text"
    }
    if ($text -notmatch 'invalid --renderer: banana') {
        throw "invalid renderer: the message is missing`n$text"
    }
    Write-Pass "invalid renderer is rejected with exit 1"
}

if (-not (Test-Path -LiteralPath $Exe)) {
    throw "FreePlots executable not found: $Exe"
}

if (-not $env:RUNNER_TEMP) {
    $env:RUNNER_TEMP = [System.IO.Path]::GetTempPath()
}

Write-Host "FreePlots GUI smoke test"
Write-Host "  executable: $((Resolve-Path -LiteralPath $Exe).Path)"
Write-Host "  temp:       $env:RUNNER_TEMP"

$port = $StartPort
try {
    Invoke-WindowCase -Name 'default renderer' -ExePath $Exe -Port $port -ExtraArgs @() -Environment @{} -Resize
} catch {
    Write-Fail $_.Exception.Message
}
$port++
try {
    Invoke-WindowCase -Name 'wgpu' -ExePath $Exe -Port $port -ExtraArgs @('--renderer', 'wgpu') -Environment @{}
} catch {
    Write-Fail $_.Exception.Message
}
$port++
try {
    Invoke-WindowCase -Name 'FREEPLOTS_RENDERER=wgpu' -ExePath $Exe -Port $port -ExtraArgs @() -Environment @{ FREEPLOTS_RENDERER = 'wgpu' }
} catch {
    Write-Fail $_.Exception.Message
}
$port++
try {
    Invoke-OptionalGlowCase -ExePath $Exe -Port $port
} catch {
    Write-Fail $_.Exception.Message
}
try {
    Invoke-InvalidRendererCase -ExePath $Exe
} catch {
    Write-Fail $_.Exception.Message
}

Write-Host ""
Write-Host "smoke test: $script:passed passed, $script:failed failed"
if ($script:failed -gt 0) {
    exit 1
}
exit 0
