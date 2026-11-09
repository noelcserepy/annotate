#!/usr/bin/env bash
# Drives a private Annotate instance on this Mac. Usage is in SKILL.md.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(git -C "$here" rev-parse --show-toplevel)
verify="$repo/target/verify"
# Its own bundle ID, so macOS never confuses a verify run with the installed app, whose
# Control Centre record a bare `annotate` binary once took over and hid its menu bar icon.
app="$verify/AnnotateVerify.app"
bin="$app/Contents/MacOS/annotate"
# Throwaway HOME, so settings and history never touch the user's own Annotate.
scratch="${TMPDIR:-/tmp}/annotate-verify"
support="$scratch/home/Library/Application Support/Annotate"
drive="$scratch/macdrive"
# Must not clash with the installed app's shortcut.
hotkey="ctrl+alt+shift+9"

run_dir() {
    [[ -f "$verify/current-mac" ]] || { echo "error no run in progress; start one with mac.sh launch" >&2; exit 1; }
    cat "$verify/current-mac"
}
pid() { cat "$scratch/pid"; }
# Our instance is up: the recorded pid is alive and runs this checkout's verify app, not a process that reused the pid.
running() { [[ -f "$scratch/pid" ]] && [[ "$(ps -o comm= -p "$(pid)" 2>/dev/null)" == "$bin" ]]; }
editor() { "$drive" windows "$(pid)" | head -1; }
front() { local p; p=$("$drive" front); echo "$p $(basename "$(ps -o comm= -p "$p")")"; }
# Keys go to whatever is in front. Refuse unless that is our instance.
require_front() {
    [[ "$("$drive" front)" == "$(pid)" ]] || { echo "error refusing: front is $(front), not annotate $(pid)"; exit 1; }
}
center() {
    local r; r=$(editor)
    [[ -n "$r" ]] || { echo "error no editor window"; exit 1; }
    read -r x y w h <<<"$r"
    cx=$((x + w / 2)) cy=$((y + h / 2))
}
state() {
    echo "user-idle-seconds $(ioreg -c IOHIDSystem | awk '/HIDIdleTime/ {print int($NF / 1000000000); exit}')"
    echo "front $(front)"
    local r; r=$(editor || true)
    echo "editor ${r:-none}"
}

cmd=${1:-}
shift || true
if [[ ! "$cmd" =~ ^(launch|doctor|cleanup)$ ]]; then
    log="$(run_dir)/log.txt"
    echo "> $cmd $*" >> "$log"
    exec > >(tee -a "$log") 2>&1
fi
case "$cmd" in
    launch)
        if running; then
            echo "error instance $(pid) from an earlier run is still up; run mac.sh cleanup" >&2
            exit 1
        fi
        cargo build --release --manifest-path "$repo/Cargo.toml"
        rm -rf "$app"
        mkdir -p "$app/Contents/MacOS"
        cp "$repo/target/release/annotate" "$bin"
        cp "$repo/Info.plist" "$app/Contents/Info.plist"
        plutil -replace CFBundleIdentifier -string com.noelcserepy.annotate.verify "$app/Contents/Info.plist"
        plutil -replace CFBundleName -string "Annotate Verify" "$app/Contents/Info.plist"
        codesign --force --sign - "$app"
        rm -rf "$scratch"
        mkdir -p "$support"
        # `launch welcome` opens the welcome window.
        welcomed=true
        [[ "${1:-}" == welcome ]] && welcomed=false
        printf '{"keys": {"capture": "%s"}, "launch_at_login": false, "welcomed": %s}\n' "$hotkey" "$welcomed" > "$support/settings.json"
        swiftc -O "$here/macdrive.swift" -o "$drive"
        dir="$verify/$(date +%Y%m%d-%H%M%S)-mac"
        mkdir -p "$dir/shots"
        echo "$dir" > "$verify/current-mac"
        HOME="$scratch/home" nohup "$bin" > "$dir/app.log" 2>&1 &
        echo $! > "$scratch/pid"
        sleep 2
        kill -0 "$(pid)" 2>/dev/null || { echo "error annotate exited; see $dir/app.log"; exit 1; }
        if grep -q '^register' "$dir/app.log"; then echo "error hotkey not registered: $(cat "$dir/app.log")"; exit 1; fi
        echo "annotate pid $(pid) hotkey $hotkey"
        echo "evidence $dir"
        ;;
    doctor)
        run_dir >/dev/null
        if running; then echo "ok annotate pid $(pid) runs $bin"; else echo "FAIL pid $(pid) is not a running $bin"; fi
        stale=$(cd "$repo" && find src build.rs Cargo.toml Cargo.lock assets -newer "$bin" | head -3)
        if [[ -n "$stale" ]]; then echo "FAIL source changed after the build ($stale); mac.sh cleanup, then launch"; else echo "ok build is newer than the source"; fi
        if grep -q '^register' "$(run_dir)/app.log"; then echo "FAIL hotkey not registered"; else echo "ok hotkey $hotkey"; fi
        state
        ;;
    state)
        state
        ;;
    # capture X1 Y1 X2 Y2: the hotkey, then a marquee drag in screen points.
    capture)
        [[ -z "$(editor)" ]] || { echo "error an Annotate window is already open; close it first"; exit 1; }
        "$drive" key "$hotkey"
        for _ in $(seq 20); do pgrep -qP "$(pid)" screencapture && break; sleep 0.25; done
        pgrep -qP "$(pid)" screencapture || { echo "error screencapture never started; is the hotkey registered?"; exit 1; }
        sleep 0.5
        "$drive" drag "$@"
        for _ in $(seq 40); do [[ -n "$(editor)" ]] && break; sleep 0.25; done
        [[ -n "$(editor)" ]] || { echo "error no editor window within 10s"; exit 1; }
        sleep 0.5
        echo "editor $(editor)"
        echo "front $(front)"
        ;;
    # Opens the app again, as double-clicking it would. With no window up, that opens Settings.
    # Refuses without a live instance, since `open` would start one with the user's real HOME.
    reopen)
        running || { echo "error annotate $(pid) is not running"; exit 1; }
        open "$app"
        sleep 1
        "$drive" windows "$(pid)" | sed 's/^/window /'
        ;;
    key)
        require_front
        "$drive" key "$1"
        sleep 0.5
        echo "front $(front)"
        ;;
    type)
        require_front
        "$drive" type "$@"
        echo "typed $*"
        ;;
    # click DX DY / drag DX1 DY1 DX2 DY2 [HOLD]: points from the editor window's center.
    click)
        require_front
        center
        "$drive" click $((cx + $1)) $((cy + $2))
        ;;
    drag)
        require_front
        center
        "$drive" drag $((cx + $1)) $((cy + $2)) $((cx + $3)) $((cy + $4)) ${5:-}
        ;;
    shot)
        path="$(run_dir)/shots/$1.png"
        if [[ "${2:-}" == editor ]]; then
            read -r x y w h <<<"$(editor)"
            screencapture -x -R "$x,$y,$w,$h" "$path"
        else
            screencapture -x "$path"
        fi
        echo "shot $path"
        ;;
    clipboard)
        "$drive" clipboard "$(run_dir)/shots"
        ;;
    # Every entry in the private history, copied into the evidence.
    fetch)
        dir=$(run_dir)
        mkdir -p "$dir/history"
        for entry in "$support/history"/*/; do
            [[ -d "$entry" ]] || continue
            echo "entry $(basename "$entry") $(ls "$entry" | xargs)"
            [[ -f "$entry/doc.json" ]] && cat "$entry/doc.json" && echo
            cp -R "${entry%/}" "$dir/history/"
        done | tee "$dir/history.txt"
        echo "evidence $dir"
        ;;
    cleanup)
        if running; then
            kill "$(pid)"
            while kill -0 "$(pid)" 2>/dev/null; do sleep 0.2; done
            echo "stopped annotate $(pid)"
        fi
        rm -rf "$scratch"
        [[ -f "$verify/current-mac" ]] && echo "evidence kept in $(cat "$verify/current-mac")" && rm "$verify/current-mac"
        ;;
    *)
        echo "usage: mac.sh launch [welcome]|doctor|state|capture X1 Y1 X2 Y2|reopen|key COMBO|type TEXT|click DX DY|drag DX1 DY1 DX2 DY2 [HOLD]|shot NAME [editor]|clipboard|fetch|cleanup" >&2
        exit 2
        ;;
esac
