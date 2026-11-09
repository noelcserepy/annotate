#!/usr/bin/env bash
# Checks mac.sh's own guarantees: the verify instance has its own bundle ID, reopening it
# opens one Settings window, and a pid reused by another process is never opened or killed.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
mac="$here/mac.sh"
pidfile="${TMPDIR:-/tmp}/annotate-verify/pid"
fail() { echo "FAIL $*"; exit 1; }

"$mac" launch | tail -1
real=$(cat "$pidfile")
lsappinfo info -only bundleid "$real" | grep -q '"com.noelcserepy.annotate.verify"' || fail "bundle id is not com.noelcserepy.annotate.verify"
echo "ok bundle id com.noelcserepy.annotate.verify"

# Settings is 440x500 points; CGWindowList reports its frame as 432x522.
for _ in 1 2; do
    windows=$("$mac" reopen)
    [[ "$windows" == "window "*" 432 522" ]] || fail "reopen should show only Settings, got: $windows"
done
echo "ok reopen shows one Settings window, twice"

sleep 60 & other=$!
echo "$other" > "$pidfile"
if "$mac" reopen >/dev/null; then fail "reopen accepted pid $other, which is not annotate"; fi
echo "ok reopen refuses a reused pid"
"$mac" cleanup >/dev/null
kill -0 "$other" || fail "cleanup killed pid $other, which is not annotate"
kill "$other" "$real"
echo "ok cleanup leaves a reused pid alone"
