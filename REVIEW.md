Rules for the pr-reviewer runs on this repository's pull requests, read from the base branch.

- The CI gates are the steps of `.github/workflows/check.yml`, which runs on every pull request.
- The app needs Screen Recording permission and a person at the screen, and the reviewer host has no one there. Do not launch it, and do not drive the desktop with synthetic input. Probe rendering the way `AGENTS.md` describes.
- Never run `scripts/bundle.sh`. It signs the build with `codesign`, and `scripts/bundle.sh install` kills the running app, replaces `/Applications/Annotate.app` and launches it. Never run `security`, `codesign` or `tccutil`.
- Never read or write `~/Library/Application Support/Annotate`.
