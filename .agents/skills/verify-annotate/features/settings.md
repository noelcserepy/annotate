# Settings

A window from the tray menu's `Settings…` item, or the welcome window's `Customize…` button, with three tabs. Appearance sets the colours, font, text size and line width of new captures, and shows a live preview. Shortcuts records the global capture shortcut and the editor keys. General picks the save folder and turns "Start app on startup" on or off. Every change persists to `settings.json` right away.

## Sub-features

- `style`: swatches, the font list and the −/+ steppers change the preview at once, and the next capture uses the new look. Captures already in history keep theirs, because `doc.json` stores the style.
- `font-search`: clicking the font field opens a list of system fonts. Typing filters it, Enter picks the first match, and Escape closes it.
- `shortcut-record`: clicking a shortcut turns it red, and the next accepted combo replaces it. Capture needs Ctrl, Alt or Cmd. Arrow and Rectangle need a single key without modifiers. Clicking the field again cancels.
- `shortcut-live`: the new capture shortcut works right away and the old one stops. Editor keys apply on the next key press.
- `save-folder`: `Choose…` opens a folder picker. `<mod>+S` in the editor writes there.
- `reset`: "Reset to defaults" restores keys, style and the save folder.
- `startup`: the toggle registers or removes the app's start at login. On the Mac that's a login item, and only an installed .app registers. On Windows it's an `Annotate` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` holding the quoted exe path. Every launch re-registers, so the entry follows the exe if it moves.

## How to get to it (user POV)

- Click the tray icon and choose `Settings…`.
- On first launch, click `Customize…` in the welcome window.

## Driving it with mac.sh / win.sh

Preconditions:

- A person has opened `Settings…` from the tray, because the harness can't open tray menus, and will close it again.

- **Current shortcut.** Open the Shortcuts tab with `<sh> click` on the tab row, then `<sh> shot settings`. Capture shows `⌃⌥⇧9` on the private mac instance and `Ctrl+Shift+4` on Windows.
- **Record.** Run `<sh> click` on the Capture field, using offsets from the window center (it sits right of center on the first row), then `<sh> key ctrl+alt+8`. The field shows the new combo, and `settings.json` holds `"capture": "ctrl+alt+8"` under `keys`.
- **Live.** Close Settings with Done, then run `capture`. `drive.ps1` reads the capture shortcut from settings on Windows. `mac.sh` always presses `ctrl+alt+shift+9`, so on the Mac re-record that combo before `capture`.
- **Style.** On Appearance, click a swatch and `+` on Text size, then `<sh> shot`. The preview shows the new colour and size. Make a callout in a new capture and compare.

## Gotchas

- Changing settings on Windows changes the user's real ones. Record the original `settings.json` first and restore it before cleanup.
- The editor helpers take the first visible Annotate window. With settings open, that may be the settings window.
- "Start app on startup" does nothing for the bare binary that `mac.sh` runs, so on the Mac a toggle that turns red proves only the UI. On Windows, check the registry with `ssh win-dev reg query HKCU\Software\Microsoft\Windows\CurrentVersion\Run /v Annotate`.
- GPUI has no slider, text field or colour picker. Sizes use −/+ steppers, colours are fixed swatches, and the font search reads raw key presses.
