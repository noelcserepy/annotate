# Copy and close

`<mod>+C` with no text selected puts the annotated image on the clipboard, closes the editor and saves the capture to history. `<mod>+S` writes the image to the save folder instead. The app keeps running in the tray, ready for the next capture. Escape (when nothing is selected) and `<mod>+W` close without copying. These are the default keys. Settings can change all of them except `<mod>+W`.

## Sub-features

- `copy-image`: the clipboard holds the annotated image in formats other apps can paste. On macOS that's PNG plus TIFF sized in points. On Windows it's `PNG` plus a DIB, which Paint, Office and chat apps read.
- `copy-closes`: the editor window closes and the app stays alive.
- `copy-text`: with text selected inside a callout, `<mod>+C` copies the text and leaves the editor open.
- `save-closes`: `<mod>+S` writes `Annotate YYYY-MM-DD HH.MM.SS.png` to the save folder (Downloads by default) and closes the editor. If the write fails, an alert says so and the editor stays open.
- `close-saves`: Escape and `<mod>+W` close and save `doc.json` and a thumbnail without copying.

## How to get to it (user POV)

- In the editor press `Cmd+C` (macOS) or `Ctrl+C` (Windows).
- Press Escape until the window closes, or `Cmd/Ctrl+W`.

## Driving it with mac.sh / win.sh

Preconditions:

- An editor is open with at least one callout (see annotate.md), and the callout is not in editing mode.

- **Before.** Run `<sh> shot before-copy editor`.
- **Copy.** Run `<sh> key <mod>+c`. The output's front app is no longer annotate.
- **Closed, still alive.** Run `<sh> state`. It must show `editor none`. On Windows it must not say `annotate not running`, and `win.sh doctor` still reports the pid.
- **Pasteable.** Run `<sh> clipboard`. On Windows: `formats` includes `PNG` and `DeviceIndependentBitmap`, `bitmap WxH` isn't `none`, and `shots/clipboard-bitmap.png` shows the annotations. On the Mac: `types` includes `public.png` and `public.tiff`, and the TIFF's pt size is the px size divided by the display scale.
- **Save to folder.** In a fresh editor run `<sh> key <mod>+s`, then `state` shows `editor none`. On the Mac the PNG is in the private instance's `$TMPDIR/annotate-verify/home/Downloads`. On Windows it's in the user's real Downloads. Delete it afterwards.
- **Saved.** Run `fetch`. The entry has `doc.json` with the callout text and a `thumb-*.png`.

## Gotchas

- `bitmap none` on Windows means apps like Paint get nothing to paste, even when `PNG` is present. GPUI's own clipboard write only sets `PNG`.
- GPUI on Windows quits the process when its last window closes. Annotate keeps a hidden window to stop that, so check the app is alive after closing.
- `<mod>+C` while editing text with a selection copies the text instead of the image.
- The clipboard read happens on the driven machine. On the Mac it overwrites the user's clipboard.
