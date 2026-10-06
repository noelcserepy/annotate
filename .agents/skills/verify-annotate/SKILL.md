---
name: verify-annotate
description: Drive the real Annotate desktop app (tray app, global capture hotkey, screenshot editor, copy to clipboard) on macOS locally and on the Windows box over ssh, and capture proof. Use to verify a change to capture, the editor, copy-and-close, history or settings on either OS.
---

# Verify Annotate

Annotate is a tray app. A global hotkey starts a region capture (`screencapture -i` on macOS, Snipping Tool on Windows), the capture opens in an editor window, and Cmd/Ctrl+C copies the annotated image and closes the window. Every capture is saved to history.

The harness takes over the real mouse and keyboard. On the Mac that is the machine the user is typing on, so check `user-idle-seconds` in the doctor output first and don't drive while the user is active. Every key and click helper refuses to act unless Annotate is in front, so stray input never lands in another app. Copy-and-close overwrites the system clipboard on the driven machine.

To wait for the Mac user to step away, poll until they've been idle for 30s, then run the whole drive in one script:

```sh
until [ "$(ioreg -c IOHIDSystem | awk '/HIDIdleTime/ {print int($NF / 1000000000); exit}')" -ge 30 ]; do sleep 5; done
```

Read `features/README.md` for the map, then the feature file for the recipe.

## Launch

macOS. A private instance from this checkout, with its own `HOME` and the hotkey `ctrl+alt+shift+9`, so it runs next to the user's installed Annotate.app without sharing settings, history or shortcut:

```sh
.agents/skills/verify-annotate/scripts/mac.sh launch
```

It builds `target/release/annotate`, starts it, and is ready when it prints `annotate pid N hotkey ctrl+alt+shift+9`. An `error` line means the instance died or the hotkey didn't register; `app.log` in the evidence dir has stderr. One mac run at a time.

Windows. There is one instance: the user's own install at `%LOCALAPPDATA%\Programs\Annotate\annotate.exe` on `win-dev` (override with `WIN_HOST`). Windows can't isolate it, so a run uses the user's real settings and history, and cleanup deletes exactly the history entries the run created.

```sh
.agents/skills/verify-annotate/scripts/win.sh deploy   # sync this checkout, cargo build on the box, swap the exe, start it
.agents/skills/verify-annotate/scripts/win.sh launch   # only start it, if doctor says it isn't running
.agents/skills/verify-annotate/scripts/win.sh begin    # open a run: marks the start time, creates the evidence dir
```

`deploy` ends with `installed <time> pid N`. `begin` refuses while another run is open.

## Doctor

Read-only. Run it first, and again whenever something looks off.

```sh
.agents/skills/verify-annotate/scripts/mac.sh doctor
.agents/skills/verify-annotate/scripts/win.sh doctor
```

Every line starting `FAIL` must be fixed before driving. It names the fix, usually `win.sh deploy` or a mac relaunch. The checks:

- macOS: the pid is alive and runs this checkout's binary, the build is newer than the source, and the hotkey registered.
- Windows: the box's source matches this checkout byte for byte, the installed exe is the latest build, and the process runs on the desktop (session > 0) and started after the install.
- Both: the last lines report `user-idle-seconds`, the front app and any open editor rect.

## Drive

The verbs are the same on both OSes. On the Mac, call `mac.sh <verb>`. On Windows, call `win.sh desktop <verb>`, which runs `scripts/drive.ps1` on the box's desktop through a one-shot scheduled task, because an ssh session can't see the desktop.

- `capture X1 Y1 X2 Y2` presses the hotkey, waits for the capture overlay and drags a marquee in screen coordinates. Units are points on the Mac and pixels on Windows. It prints the editor rect and the front app.
- `key COMBO` presses keys like `escape`, `ctrl+c`, `cmd+z` or `backspace`. Use `ctrl+` on Windows and `cmd+` on the Mac.
- `type WORDS...` types plain text into the active callout. Avoid shell or PowerShell metacharacters.
- `click DX DY` and `drag DX1 DY1 DX2 DY2 [HOLD]` take offsets from the editor window's center. HOLD is a key held for the whole drag: `a` for arrows, `r` for rects.
- `shot NAME [editor]` saves a screenshot of the whole screen or only the editor.
- `clipboard` lists the clipboard formats and saves the image payloads, which is what a paste would get.
- `state` reports idle seconds, the front app and the editor rect.

Then collect the files the app wrote:

```sh
.agents/skills/verify-annotate/scripts/mac.sh fetch
.agents/skills/verify-annotate/scripts/win.sh fetch    # also pulls the Windows screenshots
```

`fetch` prints each history entry this run created, with its `doc.json`, and copies the entries into the evidence dir.

A full copy-and-close run on Windows:

```sh
s=.agents/skills/verify-annotate/scripts
$s/win.sh doctor && $s/win.sh begin
$s/win.sh desktop capture 300 250 800 550
$s/win.sh desktop click -100 -40 && $s/win.sh desktop type verify note && $s/win.sh desktop key escape
$s/win.sh desktop shot before-copy editor
$s/win.sh desktop key ctrl+c && $s/win.sh desktop state && $s/win.sh desktop clipboard
$s/win.sh fetch && $s/win.sh cleanup
```

## Evidence

Evidence lives in `target/verify/<timestamp>-mac/` or `target/verify/<timestamp>-win/`. It's gitignored, and cleanup never touches it. `log.txt` records every drive command with its output. `shots/` holds screenshots and clipboard dumps, and `history/` holds the entries the app saved.

A proof needs:

- The real user path. Use the hotkey and the keys a user presses. Never write `doc.json` or call internals to set state up.
- The action and the result. Take a `shot` before the final key and `state` after it. The editor rect going to `none` shows the window closed, and on Windows the app must still be running.
- The side effects, checked separately from the pixels. Look at the `clipboard` formats and the saved bitmap, and at `history/<id>/doc.json` holding the text you typed.
- Your own look at the images. Read the PNGs and say what they show.

## Cleanup

```sh
.agents/skills/verify-annotate/scripts/mac.sh cleanup   # kills only the pid this run started, deletes the private HOME
.agents/skills/verify-annotate/scripts/win.sh cleanup   # see below
```

On Windows, cleanup:

- dismisses a leftover Snipping Tool overlay,
- restarts the installed app only if an editor is still open,
- deletes history entries created since `begin`,
- removes `C:\Users\Admin\annotate-verify`.

Run cleanup after failed attempts too. Both print where the evidence was kept.

## Helpers

All of these live in `scripts/`.

- `mac.sh` is the macOS entry point. It compiles `macdrive.swift` into the scratch dir on launch.
- `macdrive.swift` posts CGEvent keys and mouse events and reads CGWindowList and NSPasteboard. It runs as `$TMPDIR/annotate-verify/macdrive front|windows PID|key COMBO|type TEXT|click X Y|drag X1 Y1 X2 Y2 [HOLD]|clipboard DIR`.
- `win.sh` is the Windows entry point. It copies `box.ps1` and `drive.ps1` to the box before each call.
- `box.ps1` runs in the ssh session and handles install, launch, doctor, begin, history, cleanup, and handing desktop jobs to `drive.ps1`.
- `drive.ps1` runs on the Windows desktop and does input, screenshots and clipboard reads. It is driven by `job.txt`, `out.txt` and `done.txt` in `C:\Users\Admin\annotate-verify`.
