# Settings

A small window from the tray menu where the user records a new global capture shortcut and turns "Open at login" on or off. Both persist to `settings.json`.

## Sub-features

- `shortcut-record`: clicking the shortcut field turns it red. The next combo with Ctrl, Alt or Cmd becomes the shortcut, and Escape cancels.
- `shortcut-live`: the new shortcut works right away and the old one stops.
- `login`: the toggle registers or removes the login item. It only works for an installed macOS .app and does nothing on Windows.

## How to get to it (user POV)

- Click the tray icon and choose `Settings…`.

## Driving it with mac.sh / win.sh

Preconditions:

- A person has opened `Settings…` from the tray, because the harness can't open tray menus.

- **Current shortcut.** Run `<sh> shot settings`. The field shows the shortcut (`⌃⌥⇧9` on the private mac instance, `Ctrl+Shift+4` on Windows).
- **Record.** Run `<sh> click` on the field, using offsets from the window center (the field sits right of center on the first row), then `<sh> key ctrl+alt+8`. The field shows the new combo, and `settings.json` holds `"hotkey": "ctrl+alt+8"`.
- **Live.** Close settings with `<sh> key <mod>+w` and run `capture`. `drive.ps1` and `mac.sh` read the hotkey from settings on Windows but use the fixed `ctrl+alt+shift+9` on the Mac, so on the Mac re-record that combo before `capture`.

## Gotchas

- Changing the shortcut on Windows changes the user's real setting. Record the original first and restore it before cleanup.
- The editor helpers take the first visible Annotate window. With settings open, that may be the settings window.
- "Open at login" does nothing for the bare binary that `mac.sh` runs, or on Windows. A toggle that turns red proves only the UI.
