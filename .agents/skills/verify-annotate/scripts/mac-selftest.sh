#!/usr/bin/env bash
# Checks mac.sh's own guarantees: the verify instance has its own bundle ID, reopening it opens
# one Settings window, only a verify app from some checkout counts as the instance, and a launch
# never replaces a live one.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
mac="$here/mac.sh"
source "$mac"
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

if "$mac" launch >/dev/null 2>&1; then fail "a second launch went ahead while $real ran"; fi
echo "ok launch refuses while an instance runs"

[[ "$(verify_app "$real")" == "$app" ]] || fail "verify_app $real is not $app"
sleep 60 & other=$!
if verify_app "$other" >/dev/null; then fail "verify_app accepted pid $other, which is not annotate"; fi
# Another checkout's instance, stood in for by a signed copy of sleep at that bundle path.
elsewhere=$(mktemp -d)
mkdir -p "$elsewhere/AnnotateVerify.app/Contents/MacOS"
cp /bin/sleep "$elsewhere/AnnotateVerify.app/Contents/MacOS/annotate"
codesign --force --sign - "$elsewhere/AnnotateVerify.app/Contents/MacOS/annotate" 2>/dev/null
"$elsewhere/AnnotateVerify.app/Contents/MacOS/annotate" 60 & foreign=$!
found=$(verify_app "$foreign") || true
kill "$other" "$foreign"
wait "$other" "$foreign" 2>/dev/null || true
rm -rf "$elsewhere"
[[ "$found" == "$elsewhere/AnnotateVerify.app" ]] || fail "verify_app missed another checkout's instance, got: $found"
echo "ok only a verify app from some checkout counts as the instance"

"$mac" cleanup >/dev/null
if kill -0 "$real" 2>/dev/null; then fail "cleanup left $real running"; fi
echo "ok cleanup stops the instance"
