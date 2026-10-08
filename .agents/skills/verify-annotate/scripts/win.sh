#!/usr/bin/env bash
# Drives the installed Annotate on a Windows box over ssh. Usage is in SKILL.md.
set -euo pipefail

host=${WIN_HOST:-win-dev}
here=$(cd "$(dirname "$0")" && pwd)
repo=$(git -C "$here" rev-parse --show-toplevel)
verify="$repo/target/verify"
remote='C:\Users\Admin\annotate-verify'
src='C:\Users\Admin\src\annotate'

box() {
    ssh "$host" "New-Item -ItemType Directory -Force $remote | Out-Null"
    scp -q "$here/box.ps1" "$here/drive.ps1" "$host:C:/Users/Admin/annotate-verify/"
    ssh "$host" "powershell -NoProfile -ExecutionPolicy Bypass -File $remote\\box.ps1 $*" | tr -d '\r'
}

# Replaces the box's source with this checkout, keeping its target dir.
push_source() {
    tgz=$(mktemp -t annotate-src).tgz
    COPYFILE_DISABLE=1 tar --exclude=./target --exclude=./.git -czf "$tgz" -C "$repo" .
    ssh "$host" "New-Item -ItemType Directory -Force $src | Out-Null; Get-ChildItem $src -Exclude target | Remove-Item -Recurse -Force"
    scp -q "$tgz" "$host:C:/Users/Admin/src/annotate.tgz"
    rm "$tgz"
    ssh "$host" "tar -xzf C:\\Users\\Admin\\src\\annotate.tgz -C $src"
}

run_dir() {
    [[ -f "$verify/current-win" ]] || { echo "no run in progress; start one with win.sh begin" >&2; exit 1; }
    cat "$verify/current-win"
}

cmd=${1:-}
shift || true
case "$cmd" in
    check)
        push_source
        ssh "$host" "cd $src; cargo clippy --release --locked -- -D warnings; exit \$LASTEXITCODE"
        ;;
    deploy)
        push_source
        ssh "$host" "cd $src; cargo build --release --locked; exit \$LASTEXITCODE"
        box install
        ;;
    launch)
        box launch "$@"
        ;;
    doctor)
        out=$(box doctor)
        echo "$out" | grep -v '^src '
        local_src=$(cd "$repo" && find . -path ./target -prune -o -path ./.git -prune -o -type f \( -name '*.rs' -o -name Cargo.toml -o -name Cargo.lock \) -print |
            sed 's|^\./||' | sort | while read -r f; do echo "src $(shasum -a 256 "$f" | cut -d' ' -f1) $f"; done)
        if diff <(echo "$local_src") <(echo "$out" | grep '^src ' | sort -k3) >/dev/null; then
            echo "ok source on $host matches this checkout"
        else
            echo "FAIL source on $host differs from this checkout; run win.sh deploy"
        fi
        ;;
    begin)
        box begin
        dir="$verify/$(date +%Y%m%d-%H%M%S)-win"
        mkdir -p "$dir"
        echo "$dir" > "$verify/current-win"
        echo "evidence $dir"
        ;;
    desktop)
        dir=$(run_dir)
        echo "> $*" >> "$dir/log.txt"
        status=0
        box desktop "$@" | tee -a "$dir/log.txt" || status=$?
        exit "$status"
        ;;
    fetch)
        dir=$(run_dir)
        scp -q -r "$host:C:/Users/Admin/annotate-verify/shots" "$dir/" 2>/dev/null || true
        box history | tee "$dir/history.txt"
        mkdir -p "$dir/history"
        for id in $(grep '^entry ' "$dir/history.txt" | cut -d' ' -f2); do
            scp -q -r "$host:C:/Users/Admin/AppData/Roaming/Annotate/history/$id" "$dir/history/"
        done
        echo "evidence $dir"
        ;;
    cleanup)
        box cleanup
        ssh "$host" "Remove-Item -Recurse -Force $remote"
        [[ -f "$verify/current-win" ]] && echo "evidence kept in $(cat "$verify/current-win")" && rm "$verify/current-win"
        ;;
    *)
        echo "usage: win.sh check|deploy|launch [welcome]|doctor|begin|desktop <action>...|fetch|cleanup" >&2
        exit 2
        ;;
esac
