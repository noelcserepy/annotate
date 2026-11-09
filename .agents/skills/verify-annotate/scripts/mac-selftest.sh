#!/usr/bin/env bash
# Checks mac.sh's own guarantees: the verify instance has its own bundle ID, reopening it
# opens one Settings window, a pid reused by another process is never opened or killed, and
# another checkout's live instance is never wiped by a launch.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
mac="$here/mac.sh"
# mac.sh's pid record. Writing it is the only way to stage a reused pid or another checkout's instance.
pidfile="${TMPDIR:-/tmp}/annotate-verify/pid"
fail() { echo "FAIL $*"; exit 1; }

out=$("$mac" launch)
real=$(sed -n 's/^annotate pid \([0-9]*\) .*/\1/p' <<<"$out")
[[ -n "$real" ]] || fail "launch printed no pid: $out"
lsappinfo info -only bundleid "$real" | grep -q '"com.noelcserepy.annotate.verify"' || fail "bundle id is not com.noelcserepy.annotate.verify"
echo "ok bundle id com.noelcserepy.annotate.verify"

# Settings is 440x500 points; CGWindowList reports its frame as 432x522. The digit classes
# never match a newline, so a second window fails the match.
for _ in 1 2; do
    windows=$("$mac" reopen)
    [[ "$windows" =~ ^window\ -?[0-9]+\ -?[0-9]+\ 432\ 522$ ]] || fail "reopen should show only Settings, got: $windows"
done
echo "ok reopen shows one Settings window, twice"

sleep 60 & other=$!
echo "$other" > "$pidfile"
if "$mac" reopen >/dev/null; then fail "reopen accepted pid $other, which is not annotate"; fi
echo "ok reopen refuses a reused pid"
"$mac" cleanup >/dev/null
kill -0 "$other" || fail "cleanup killed pid $other, which is not annotate"
kill "$other" "$real"; wait "$other" 2>/dev/null || true
echo "ok cleanup leaves a reused pid alone"

# A live verify app from another checkout, stood in for by sleep at that bundle path.
elsewhere=$(mktemp -d)
mkdir -p "$elsewhere/AnnotateVerify.app/Contents/MacOS"
cp /bin/sleep "$elsewhere/AnnotateVerify.app/Contents/MacOS/annotate"
codesign --force --sign - "$elsewhere/AnnotateVerify.app/Contents/MacOS/annotate" 2>/dev/null
"$elsewhere/AnnotateVerify.app/Contents/MacOS/annotate" 60 & foreign=$!
mkdir -p "$(dirname "$pidfile")"
echo "$foreign" > "$pidfile"
if "$mac" launch >/dev/null 2>&1; then fail "launch went ahead while another checkout's instance ran"; fi
echo "ok launch refuses while another checkout's instance runs"
"$mac" cleanup >/dev/null
if kill -0 "$foreign" 2>/dev/null; then fail "cleanup left another checkout's instance running"; fi
rm -rf "$elsewhere"
echo "ok cleanup stops another checkout's instance"
