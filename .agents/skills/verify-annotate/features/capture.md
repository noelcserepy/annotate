# Capture

The user presses the global shortcut, drags a region with the OS capture tool, and the region opens in an editor window in front of other apps. Cancelling the capture opens nothing.

## Sub-features

- `capture-hotkey` starts a capture from the global shortcut.
- `capture-tray` starts a capture from the tray menu's `Capture` item.
- `capture-editor` opens the editor in front, sized to the region.
- `capture-cancel` makes Escape during the marquee open nothing and leave the app usable.

## How to get to it (user POV)

- Press the shortcut: `Cmd+Shift+4` on macOS by default, `Ctrl+Shift+4` on Windows.
- Click the tray icon and choose `Capture`.

## Driving it with mac.sh / win.sh

Preconditions:

- Doctor is clean, no editor is open, and on Windows `win.sh begin` has run.

- **Hotkey capture.** Press the shortcut and drag a region. Run `<sh> capture 300 250 800 550`. The output has `editor X1 Y1 X2 Y2` and `front annotate` (on the Mac, the front pid is the instance's).
- **Editor shows the region.** Run `<sh> shot captured editor`. The PNG shows the dragged region inside the editor window.
- **Saved original.** Run the `fetch` command. A new entry lists `original.png`.
- **Cancel.** Press the shortcut, then Escape over the overlay. On Windows, run `win.sh desktop settle` after the overlay comes up. On the Mac, `macdrive key escape`. `state` shows `editor none`, and a second `capture` still works afterwards.

## Gotchas

- Windows uses Snipping Tool (`ms-screenclip:`). A cancelled snip leaves Annotate waiting up to 60 s for the clipboard, and during that time the shortcut does nothing.
- Windows only notices a new capture when the clipboard image changes. Capturing the exact same pixels twice in a row looks like a cancel.
- On the Mac, `screencapture -i` needs Screen Recording permission for the app running the shell. Without it the capture comes back as wallpaper only, so check the shot.
- The tray `Capture` item can't be driven by this harness. Report `capture-tray` as skipped unless a person clicks it.
- If another app owns the shortcut (Greenshot used to own Ctrl+Shift+4 on win-dev), nothing happens and `capture` fails with "overlay never came up".
