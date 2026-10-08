//! Renders assets/annotate-logo.svg into the app icon and the tray icon. The app icon
//! is `AppIcon.icns` on macOS, which scripts/bundle.sh copies into the bundle, and an
//! icon resource linked into the exe on Windows. The tray icon is a template image on
//! macOS and the logo itself elsewhere.

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

    let os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    // 18pt at 2x.
    let rgba = if os == "macos" {
        app_icon(&tree, out);
        menubar_template(&render(&tree, 36, 30.))
    } else {
        if os == "windows" {
            exe_icon(&tree, out);
        }
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

/// Explorer shows icon resource 1 as the exe's icon, and GPUI loads it for its windows.
fn exe_icon(tree: &usvg::Tree, out: &Path) {
    let sizes = [16, 24, 32, 48, 64, 256];
    let pngs: Vec<Vec<u8>> = sizes.iter().map(|&size| render(tree, size, size as f32).encode_png().unwrap()).collect();
    // ICO: a 6-byte header, a 16-byte entry per image, then the images as PNGs.
    let mut ico = [0, 0, 1, 0].to_vec();
    ico.extend((sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len();
    for (size, png) in sizes.iter().zip(&pngs) {
        // A side of 256 is written as 0.
        let side = (size % 256) as u8;
        ico.extend([side, side, 0, 0]);
        ico.extend(1u16.to_le_bytes());
        ico.extend(32u16.to_le_bytes());
        ico.extend((png.len() as u32).to_le_bytes());
        ico.extend((offset as u32).to_le_bytes());
        offset += png.len();
    }
    pngs.iter().for_each(|png| ico.extend(png));
    let ico_path = out.join("annotate.ico");
    fs::write(&ico_path, ico).unwrap();
    let rc = out.join("annotate.rc");
    fs::write(&rc, format!("1 ICON \"{}\"\n", ico_path.display().to_string().replace('\\', "\\\\"))).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE).manifest_required().unwrap();
}

/// The logo scaled to `content` pixels, centered on a `size` square canvas.
fn render(tree: &usvg::Tree, size: u32, content: f32) -> Pixmap {
    let mut pixmap = Pixmap::new(size, size).unwrap();
    let scale = content / tree.size().width().max(tree.size().height());
    let offset = (size as f32 - content) / 2.;
    resvg::render(tree, Transform::from_scale(scale, scale).post_translate(offset, offset), &mut pixmap.as_mut());
    pixmap
}
