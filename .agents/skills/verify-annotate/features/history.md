# History

Every capture is kept. The tray menu lists the five most recent with a thumbnail and date, and choosing one reopens it in the editor with its annotations, ready to edit and copy again.

## Sub-features

- `history-save`: closing an editor writes `original.png`, `doc.json` and `thumb-<millis>.png` under the history dir.
- `history-menu`: the tray menu shows the newest five entries, newest first.
- `history-reopen`: choosing an entry opens it with its saved callouts. Choosing an entry that's already open focuses its window.

## How to get to it (user POV)

- Click the tray icon and pick a capture under `Capture`.

## Driving it with mac.sh / win.sh

Preconditions:

- At least one capture made in this run and closed.

- **Saved files.** Run `fetch`. Each entry lists `doc.json original.png thumb-*.png`, and `doc.json` matches what was drawn.
- **Thumbnail.** Open `history/<id>/thumb-*.png` from the evidence dir. It shows the annotated image.
- **Reopen.** Can't be driven. See Gotchas.

## Gotchas

- The tray menu is drawn by the OS (NSStatusItem, Windows notification area). The harness can't open it, so report `history-menu` and `history-reopen` as skipped unless a person clicks.
- History lives in `~/Library/Application Support/Annotate/history` on the Mac (the private `HOME` during a run) and `%APPDATA%\Annotate\history` on Windows. Entry ids are zero-padded epoch milliseconds.
- On Windows the history is the user's real one. Only `win.sh cleanup` deletes from it, and only entries newer than `begin`.
