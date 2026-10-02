# annotate

- CI runs `cargo fmt --check` and `cargo clippy --release --locked -- -D warnings`. There are no tests.
- To check layout or rendering, write a probe in `src/bin/probe.rs` that pulls in `#[path = "../doc.rs"] mod doc;` and `#[path = "../render.rs"] mod render;`, builds a `Doc`, renders it with `render::compose` and saves the PNG. Look at the PNG, then delete the probe. Only a person at the screen can drive the editor itself.
- GPUI has no docs. Read its source in `~/.cargo/registry/src/*/gpui-0.2.2/`. Style methods like `cursor_*` are generated in `gpui-macros`.
