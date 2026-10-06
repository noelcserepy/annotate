//! Renders assets/annotate-logo.svg into the app icon (`AppIcon.icns`, which
//! scripts/bundle.sh copies into the bundle) and the tray icon: a template image on
//! macOS, the logo itself elsewhere.

use std::{env, fs, path::Path, process::Command};

use resvg::{
    tiny_skia::{Pixmap, Transform},
    usvg,
};

const LOGO: &str = "assets/annotate-logo.svg";

fn main() {
    println!("cargo:rerun-if-changed={LOGO}");
    let out = env::var("OUT_DIR").unwrap();
    let out = Path::new(&out);
    let tree = usvg::Tree::from_data(&fs::read(LOGO).unwrap(), &usvg::Options::default()).unwrap();

    // 18pt at 2x.
    let rgba = if env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
        app_icon(&tree, out);
        menubar_template(&render(&tree, 36, 30.))
    } else {
        render(&tree, 36, 36.)
            .pixels()
            .iter()
            .flat_map(|p| {
                let c = p.demultiply();
                [c.red(), c.green(), c.blue(), c.alpha()]
            })
            .collect()
    };
    fs::write(out.join("tray.rgba"), rgba).unwrap();
}

/// Templates only use alpha, so dark parts become ink and light parts become holes.
fn menubar_template(pixmap: &Pixmap) -> Vec<u8> {
    let ink: Vec<f32> = pixmap
        .pixels()
        .iter()
        .map(|p| {
            let c = p.demultiply();
            let luma = (0.2126 * c.red() as f32 + 0.7152 * c.green() as f32 + 0.0722 * c.blue() as f32) / 255.;
            c.alpha() as f32 / 255. * (1. - luma)
        })
        .collect();
    let max = ink.iter().cloned().fold(f32::EPSILON, f32::max);
    ink.iter().flat_map(|i| [0, 0, 0, (i / max * 255.).round() as u8]).collect()
}

fn app_icon(tree: &usvg::Tree, out: &Path) {
    // macOS icon grid: the shape fills 824 of 1024 points, leaving room for the shadow.
    let iconset = out.join("AppIcon.iconset");
    fs::create_dir_all(&iconset).unwrap();
    for points in [16, 32, 128, 256, 512] {
        for scale in [1, 2] {
            let size = points * scale;
            let suffix = if scale == 2 { "@2x" } else { "" };
            let path = iconset.join(format!("icon_{points}x{points}{suffix}.png"));
            render(tree, size, size as f32 * 824. / 1024.).save_png(path).unwrap();
        }
    }
    let status = Command::new("iconutil").arg("-c").arg("icns").arg(&iconset).arg("-o").arg(out.join("AppIcon.icns")).status();
    assert!(status.is_ok_and(|s| s.success()), "iconutil failed");
}

/// The logo scaled to `content` pixels, centered on a `size` square canvas.
fn render(tree: &usvg::Tree, size: u32, content: f32) -> Pixmap {
    let mut pixmap = Pixmap::new(size, size).unwrap();
    let scale = content / tree.size().width().max(tree.size().height());
    let offset = (size as f32 - content) / 2.;
    resvg::render(tree, Transform::from_scale(scale, scale).post_translate(offset, offset), &mut pixmap.as_mut());
    pixmap
}
