# annotate

- There is no CI and there are no tests. Before pushing, run `cargo fmt --check` and `cargo clippy --release --locked -- -D warnings`. That clippy only covers the macOS build, so unless a change is mac-only, also run `.agents/skills/verify-annotate/scripts/win.sh check`, which runs it on win-dev.
- To check layout or rendering, write a probe in `src/bin/probe.rs` that pulls in `#[path = "../doc.rs"] mod doc;` and `#[path = "../render.rs"] mod render;`, builds a `Doc`, renders it with `render::compose` and saves the PNG. Look at the PNG, then delete the probe.
- GPUI has no docs. Read its source in `~/.cargo/registry/src/*/gpui-0.2.2/`. Style methods like `cursor_*` are generated in `gpui-macros`.
