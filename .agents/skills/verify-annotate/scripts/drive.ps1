# Runs on the Windows desktop, started by box.ps1 through a one-shot scheduled task.
# Reads one action from job.txt, writes its output to out.txt, then done.txt.
# SSH sessions can't see the desktop, its foreground window or its clipboard; this can.

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$shots = Join-Path $root "shots"
New-Item -ItemType Directory -Force $shots | Out-Null
$out = New-Object System.Collections.Generic.List[string]
function say($line) { $out.Add([string]$line) }

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public struct RECT { public int L, T, R, B; }
public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
public class U {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern void keybd_event(byte k, byte s, uint f, UIntPtr e);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int x, int y, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint type);
  [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LASTINPUTINFO i);
}
"@

function fg-proc {
    $p = 0; [U]::GetWindowThreadProcessId([U]::GetForegroundWindow(), [ref]$p) | Out-Null
    Get-Process -Id $p -ErrorAction SilentlyContinue
}

function annotate-pid {
    $p = Get-Process annotate -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $p) { throw "annotate is not running" }
    $p.Id
}

# Annotate's first visible window of class `$class`.
function window-rect($class) {
    $ap = annotate-pid
    $script:found = $null
    [U]::EnumWindows({ param($h, $l)
        $p = 0; [U]::GetWindowThreadProcessId($h, [ref]$p) | Out-Null
        if ($p -eq $ap -and [U]::IsWindowVisible($h)) {
            $c = New-Object Text.StringBuilder 64; [U]::GetClassName($h, $c, 64) | Out-Null
            if ($c.ToString() -eq $class) { $r = New-Object RECT; [U]::GetWindowRect($h, [ref]$r) | Out-Null; $script:found = $r }
        }
        $true }, [IntPtr]::Zero) | Out-Null
    $script:found
}

# The editor is a GPUI window. The settings and welcome windows have the same class, so
# close them before driving the editor.
function editor-rect { window-rect "Zed::Window" }

# The capture overlay covers every monitor until the user drags or cancels.
function overlay-up { [bool](window-rect "AnnotateCapture") }

function wait-editor($seconds) {
    for ($i = 0; $i -lt $seconds * 4; $i++) {
        $r = editor-rect
        if ($r) { return $r }
        Start-Sleep -Milliseconds 250
    }
    $null
}

# Keys go to whatever is in front. Refuse unless that is Annotate.
function require-annotate-front {
    $f = fg-proc
    if (-not $f -or $f.ProcessName -ne "annotate") { throw "refusing: foreground is '$($f.ProcessName)', not annotate" }
}

$vk = @{ ctrl = 0x11; shift = 0x10; alt = 0x12; cmd = 0x5B; escape = 0x1B; enter = 0x0D; backspace = 0x08; delete = 0x2E; tab = 0x09; space = 0x20 }
function vk-of($name) {
    if ($vk.ContainsKey($name)) { return $vk[$name] }
    if ($name.Length -eq 1) { return [int][char]$name.ToUpper() }
    throw "unknown key '$name'"
}
function key-down($code) { [U]::keybd_event($code, [U]::MapVirtualKey($code, 0), 0, [UIntPtr]::Zero) }
function key-up($code) { [U]::keybd_event($code, [U]::MapVirtualKey($code, 0), 2, [UIntPtr]::Zero) }
# "ctrl+shift+4": hold every modifier, tap the last key, release in reverse.
function press($combo) {
    $codes = $combo.ToLower().Split("+") | ForEach-Object { vk-of $_ }
    $codes | ForEach-Object { key-down $_; Start-Sleep -Milliseconds 30 }
    [array]::Reverse($codes)
    $codes | ForEach-Object { key-up $_; Start-Sleep -Milliseconds 30 }
}

function mouse-drag($x1, $y1, $x2, $y2) {
    [U]::SetCursorPos($x1, $y1) | Out-Null; Start-Sleep -Milliseconds 200
    [U]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
    for ($i = 1; $i -le 20; $i++) {
        [U]::SetCursorPos([int]($x1 + ($x2 - $x1) * $i / 20), [int]($y1 + ($y2 - $y1) * $i / 20)) | Out-Null
        Start-Sleep -Milliseconds 25
    }
    [U]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 300
}

function center($r) { @([int](($r.L + $r.R) / 2), [int](($r.T + $r.B) / 2)) }

function save-shot($name, $rect) {
    if ($rect) { $x = $rect.L; $y = $rect.T; $w = $rect.R - $rect.L; $h = $rect.B - $rect.T }
    else { $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds; $x = $b.X; $y = $b.Y; $w = $b.Width; $h = $b.Height }
    $bmp = New-Object System.Drawing.Bitmap $w, $h
    [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($x, $y, 0, 0, $bmp.Size)
    $path = Join-Path $shots "$name.png"
    $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
    say "shot $path"
}

function hotkey {
    $settings = Join-Path $env:APPDATA "Annotate\settings.json"
    # Settings that never changed the shortcut don't list it.
    $keys = if (Test-Path $settings) { (Get-Content $settings -Raw | ConvertFrom-Json).keys.capture }
    if ($keys) { $keys } else { "ctrl+shift+4" }
}

$job = (Get-Content (Join-Path $root "job.txt") -Raw).Trim().Split(" ")
$action = $job[0]
$a = $job[1..($job.Length)]
try {
    switch ($action) {
        "state" {
            $info = New-Object LASTINPUTINFO; $info.cbSize = 8; [U]::GetLastInputInfo([ref]$info) | Out-Null
            say "user-idle-seconds $([int](([Environment]::TickCount - $info.dwTime) / 1000))"
            say "foreground $((fg-proc).ProcessName)"
            if (-not (Get-Process annotate -ErrorAction SilentlyContinue)) { say "annotate not running" }
            elseif ($r = editor-rect) { say "editor $($r.L) $($r.T) $($r.R) $($r.B)" }
            else { say "editor none" }
        }
        # capture X1 Y1 X2 Y2: the user's hotkey, then a marquee drag in screen pixels.
        "capture" {
            if (editor-rect) { throw "an Annotate window is already open; close it first" }
            press (hotkey)
            for ($i = 0; $i -lt 20 -and -not (overlay-up); $i++) { Start-Sleep -Milliseconds 100 }
            if (-not (overlay-up)) { throw "capture overlay never came up (foreground: $((fg-proc).ProcessName)); is the hotkey registered?" }
            mouse-drag ([int]$a[0]) ([int]$a[1]) ([int]$a[2]) ([int]$a[3])
            $r = wait-editor 10
            if (-not $r) { throw "no editor window within 10s" }
            Start-Sleep -Milliseconds 500
            say "editor $($r.L) $($r.T) $($r.R) $($r.B)"
            say "foreground $((fg-proc).ProcessName)"
        }
        "key" { require-annotate-front; press $a[0]; Start-Sleep -Milliseconds 500; say "foreground $((fg-proc).ProcessName)" }
        "type" {
            require-annotate-front
            $text = ($a -join " ")
            foreach ($ch in $text.ToCharArray()) { [System.Windows.Forms.SendKeys]::SendWait(($ch -replace '([+^%~(){}\[\]])', '{$1}')); Start-Sleep -Milliseconds 20 }
            say "typed $text"
        }
        # click DX DY / drag DX1 DY1 DX2 DY2 [HOLD]: offsets from the editor window's center.
        "click" {
            require-annotate-front
            $c = center (editor-rect)
            mouse-drag ($c[0] + [int]$a[0]) ($c[1] + [int]$a[1]) ($c[0] + [int]$a[0]) ($c[1] + [int]$a[1])
        }
        "drag" {
            require-annotate-front
            $c = center (editor-rect)
            if ($a.Length -gt 4 -and $a[4]) { $hold = vk-of $a[4]; key-down $hold; Start-Sleep -Milliseconds 150 }
            mouse-drag ($c[0] + [int]$a[0]) ($c[1] + [int]$a[1]) ($c[0] + [int]$a[2]) ($c[1] + [int]$a[3])
            if ($hold) { key-up $hold }
        }
        "shot" { save-shot $a[0] $(if ($a[1] -eq "editor") { editor-rect } else { $null }) }
        # What a paste would see. Native apps (Paint, Word, Explorer) read the bitmap
        # formats; browsers and Electron apps also accept "PNG".
        "clipboard" {
            $data = [System.Windows.Forms.Clipboard]::GetDataObject()
            say "formats $(($data.GetFormats() -join ','))"
            $img = [System.Windows.Forms.Clipboard]::GetImage()
            if ($img) { $path = Join-Path $shots "clipboard-bitmap.png"; $img.Save($path); say "bitmap $($img.Width)x$($img.Height) $path" } else { say "bitmap none" }
            if ($data.GetDataPresent("PNG")) {
                $ms = $data.GetData("PNG"); $path = Join-Path $shots "clipboard-png.png"
                [IO.File]::WriteAllBytes($path, $ms.ToArray()); say "png $((Get-Item $path).Length) bytes $path"
            } else { say "png none" }
        }
        # Dismiss a capture overlay a failed capture left behind.
        "settle" {
            if (overlay-up) { press "escape"; say "dismissed capture overlay" }
        }
        default { throw "unknown action '$action'" }
    }
    say "ok"
} catch {
    say "error $($_.Exception.Message)"
}
$out | Set-Content (Join-Path $root "out.txt")
Set-Content (Join-Path $root "done.txt") "done"
