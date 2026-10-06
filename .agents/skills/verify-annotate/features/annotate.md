# Annotate

In the editor the user adds red callouts with text, arrows and rectangles, selects and moves them, deletes them, and undoes mistakes.

## Sub-features

- `callout-point`: a click on the image adds a callout pointing at that spot and starts text entry.
- `callout-rect`: a drag on the image adds a callout pointing at a highlighted rectangle.
- `text`: typing, including spaces, word jumps with Alt (Mac) or Ctrl (Windows) plus arrows, and select-all.
- `arrow`: hold `A` and drag to add a standalone arrow.
- `rect`: hold `R` and drag to add a standalone rectangle.
- `select-delete`: Escape commits text and leaves the callout selected. Backspace or Delete removes the selected item.
- `undo`: `<mod>+z` undoes the last change.

## How to get to it (user POV)

- Capture a region, then use the mouse and keyboard inside the editor.

## Driving it with mac.sh / win.sh

Preconditions:

- An editor is open from `<sh> capture ...`.

- **Point callout.** Click the image and type. Run `<sh> click -100 -40` and `<sh> type verify note`, then `<sh> key escape`. `<sh> shot callout editor` shows a red box reading "verify note" with an arrow to the click.
- **Rect callout.** Drag on empty image. Run `<sh> drag -150 0 -50 60`, then `<sh> type boxed` and `<sh> key escape`. The shot shows a red rectangle with a callout.
- **Arrow.** Run `<sh> drag 0 0 120 60 a`. The shot shows a standalone arrow.
- **Rect.** Run `<sh> drag 40 -60 140 0 r`. The shot shows a standalone rectangle.
- **Delete.** Select an item by clicking it, then run `<sh> key backspace`. It disappears from the shot.
- **Undo.** Run `<sh> key <mod>+z`. The last change comes back.
- **Stored.** Close with `<sh> key escape` (pressed until `editor none`), then `fetch`. `doc.json` lists `callouts` with the typed `text`, plus the `arrows` and `rects`.

## Gotchas

- Text typed while a callout is selected but not in editing mode starts editing. On Windows, a space typed at that moment doesn't start editing.
- `a` and `r` only act as tools while held with no modifier and no text being edited. Typed into a callout, they're letters.
- Keys pressed mid-drag are ignored on purpose.
- The first Escape after typing commits and selects. The next deselects, and the one after that closes the editor.
- On the Mac, `type` looks each character up in the current keyboard layout. This Mac uses QWERTZ, so fixed key codes would type z for y. A character that takes more than one keystroke, such as an accent or emoji, fails with `no key types`.
