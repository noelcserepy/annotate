# Annotate verification map

This directory is the maintained list of what a user can do in Annotate and how to prove each part on macOS and Windows. Read it before driving, then follow the matching feature file. `../SKILL.md` explains the harness verbs.

## Baseline preconditions

- macOS: a private instance from `mac.sh launch`, with hotkey `ctrl+alt+shift+9` and an empty history.
- Windows: the installed app on `win-dev`, freshly deployed. `win.sh begin` must have opened the run.
- `doctor` prints no `FAIL` line, the editor is `none`, and `user-idle-seconds` shows that nobody is using the machine.

## Driving conventions

- Write `<sh>` for `mac.sh` or `win.sh desktop`. Write `<mod>` for `cmd` on the Mac and `ctrl` on Windows.
- Start each recipe with no editor open. Close a leftover editor with `<sh> key escape`, pressed until `state` shows `editor none`.
- Use offsets from the editor center for clicks and drags inside the editor. Window positions change from run to run.
- Never drive the user's own Annotate.app on the Mac. On Windows, only `cleanup` deletes entries, and only the ones created after `begin`.

## Proof and skip reporting

- Capture the action and the result. Take a `shot` before the decisive key and run `state` or `clipboard` after it.
- Back up visible results with stored ones. That means `fetch` output (`doc.json`) and clipboard dumps.
- Say which OS and which entry point each artifact came from.
- Report a tray menu path as skipped (see Gotchas in each file). Never count a hotkey run as proof of a tray path.

## Feature entry contract

Each feature file has an H1, one paragraph, then the H2s `Sub-features`, `How to get to it (user POV)`, `Driving it with mac.sh / win.sh` (opening with `Preconditions:`) and `Gotchas`, in that order.

## Features

- [Capture](./capture.md) covers the hotkey and tray capture, the region marquee, cancelling, and the editor opening in front.
- [Annotate](./annotate.md) covers callouts by click or marquee, typing, held A and R tools, selection, delete and undo.
- [Copy and close](./copy-and-close.md) covers Cmd/Ctrl+C to the clipboard, Esc and Cmd/Ctrl+W closing, the app staying alive, and the save to history.
- [History](./history.md) covers recent captures in the tray menu and reopening one with its annotations.
- [Settings](./settings.md) covers recording a new capture shortcut and the "Open at login" toggle.
