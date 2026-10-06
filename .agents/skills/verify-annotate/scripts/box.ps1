# Runs over ssh on the Windows box, one action per call. win.sh copies it next to
# drive.ps1 in C:\Users\Admin\annotate-verify before every call.

param([string]$Action)

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = "C:\Users\Admin\src\annotate"
$built = "$src\target\release\annotate.exe"
$installed = "$env:LOCALAPPDATA\Programs\Annotate\annotate.exe"
$history = "$env:APPDATA\Annotate\history"
$started = Join-Path $root "started.txt"

function instance { Get-Process annotate -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $installed } }

# Processes started from ssh live in a hidden session with no desktop. A one-shot
# interactive scheduled task runs in the signed-in user's session instead.
function in-desktop($command) {
    schtasks /Create /F /TN AnnotateVerify /TR $command /SC ONCE /ST 00:00 /IT 2>$null | Out-Null
    schtasks /Run /TN AnnotateVerify | Out-Null
}
function end-desktop { schtasks /Delete /F /TN AnnotateVerify | Out-Null }

function launch {
    in-desktop $installed
    for ($i = 0; $i -lt 20 -and -not (instance); $i++) { Start-Sleep -Milliseconds 250 }
    end-desktop
}

function desktop([string]$job) {
    Remove-Item (Join-Path $root "done.txt"), (Join-Path $root "out.txt") -ErrorAction SilentlyContinue
    Set-Content (Join-Path $root "job.txt") $job
    in-desktop "powershell -NoProfile -STA -WindowStyle Hidden -ExecutionPolicy Bypass -File $root\drive.ps1"
    for ($i = 0; $i -lt 240 -and -not (Test-Path (Join-Path $root "done.txt")); $i++) { Start-Sleep -Milliseconds 250 }
    end-desktop
    if (-not (Test-Path (Join-Path $root "done.txt"))) { return "error drive.ps1 did not finish within 60s" }
    Get-Content (Join-Path $root "out.txt")
}

function since {
    if (-not (Test-Path $started)) { return @() }
    $from = [long](Get-Content $started)
    Get-ChildItem $history -Directory -ErrorAction SilentlyContinue | Where-Object { [long]$_.Name -ge $from } | Sort-Object Name
}

switch ($Action) {
    # Swap in the freshly built exe and start it on the desktop.
    "install" {
        $ErrorActionPreference = "Stop"
        instance | ForEach-Object { $_.Kill(); $_.WaitForExit() }
        New-Item -ItemType Directory -Force (Split-Path $installed) | Out-Null
        Copy-Item $built $installed -Force
        $lnk = "$env:APPDATA\Microsoft\Windows\Start Menu\Programs\Annotate.lnk"
        $s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk); $s.TargetPath = $installed; $s.Save()
        $ErrorActionPreference = "Continue"
        launch
        $p = instance
        if (-not $p) { "error installed exe did not start"; exit 1 }
        "installed $((Get-Item $installed).LastWriteTime.ToString('s')) pid $($p.Id)"
    }
    "launch" {
        if (-not (instance)) { launch }
        $p = instance
        if (-not $p) { "error installed exe did not start"; exit 1 }
        "running pid $($p.Id)"
    }
    # Read-only. Prints source hashes for win.sh to compare with the local checkout.
    "doctor" {
        Get-ChildItem $src -Recurse -File -Include *.rs, Cargo.toml, Cargo.lock |
            Where-Object { $_.FullName -notlike "$src\target\*" } |
            ForEach-Object { "src $((Get-FileHash $_.FullName).Hash.ToLower()) $($_.FullName.Substring($src.Length + 1).Replace('\', '/'))" }
        $newest = (Get-ChildItem $src -Recurse -File | Where-Object { $_.FullName -notlike "$src\target\*" } | Measure-Object LastWriteTime -Maximum).Maximum
        if (-not (Test-Path $built)) { "FAIL no build at $built" }
        elseif ((Get-Item $built).LastWriteTime -lt $newest) { "FAIL build is older than the synced source; run win.sh deploy" }
        elseif ((Get-FileHash $built).Hash -ne (Get-FileHash $installed -ErrorAction SilentlyContinue).Hash) { "FAIL installed exe is not the latest build; run win.sh deploy" }
        else { "ok installed exe is the latest build ($((Get-Item $installed).LastWriteTime.ToString('s')))" }
        $p = instance
        if (-not $p) { "FAIL annotate is not running from $installed" }
        elseif ($p.SessionId -eq 0) { "FAIL annotate runs in session 0, not on the desktop" }
        elseif ($p.StartTime -lt (Get-Item $installed).LastWriteTime) { "FAIL running process predates the installed exe" }
        else { "ok annotate pid $($p.Id) on the desktop" }
        $settings = "$env:APPDATA\Annotate\settings.json"
        "hotkey $(if (Test-Path $settings) { (Get-Content $settings -Raw | ConvertFrom-Json).hotkey } else { 'ctrl+shift+4 (default)' })"
        if (Test-Path $started) { "run in progress since $(Get-Content $started)" }
        desktop "state"
    }
    "begin" {
        if (Test-Path $started) { "error a run started at $(Get-Content $started) is still in progress; finish it with win.sh cleanup"; exit 1 }
        Remove-Item (Join-Path $root "shots") -Recurse -ErrorAction SilentlyContinue
        $now = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        Set-Content $started $now
        "started $now"
    }
    "desktop" {
        $out = desktop ($args -join " ")
        $out
        if ($out -match "^error") { exit 1 }
    }
    # History entries this run created, with their doc.json.
    "history" {
        since | ForEach-Object {
            "entry $($_.Name) $((Get-ChildItem $_.FullName -File | ForEach-Object Name) -join ' ')"
            $doc = Join-Path $_.FullName "doc.json"
            if (Test-Path $doc) { Get-Content $doc -Raw }
        }
    }
    # Leaves the app as the user had it: no snip overlay, no editor, no entries from this run.
    "cleanup" {
        desktop "settle"
        $state = desktop "state"
        if ($state -match "^editor \d") {
            instance | ForEach-Object { $_.Kill(); $_.WaitForExit() }
            launch
            "restarted annotate to close the editor"
        }
        since | ForEach-Object { Remove-Item $_.FullName -Recurse -Force; "removed history $($_.Name)" }
        Remove-Item $started -ErrorAction SilentlyContinue
    }
    default { "error unknown action '$Action'"; exit 1 }
}
