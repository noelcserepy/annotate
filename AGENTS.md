# annotate

- There are no tests. CI runs `cargo fmt --check` and `cargo clippy --release --locked -- -D warnings` on macOS and Windows for every PR. Run both locally before pushing.
- To check layout or rendering, write a probe in `src/bin/probe.rs` that pulls in `#[path = "../doc.rs"] mod doc;` and `#[path = "../render.rs"] mod render;`, builds a `Doc`, renders it with `render::compose` and saves the PNG. Look at the PNG, then delete the probe.
- GPUI has no docs. Read its source in `~/.cargo/registry/src/*/gpui-0.2.2/`. Style methods like `cursor_*` are generated in `gpui-macros`.
- To release, set the same version in `Cargo.toml` and `Info.plist`, then push a `vX.Y.Z` tag. `.github/workflows/release.yml` builds both apps and publishes the release.
