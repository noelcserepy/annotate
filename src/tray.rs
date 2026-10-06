use std::path::Path;

use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{self, IconMenuItem, Menu, MenuItem, PredefinedMenuItem},
};

use crate::{Command, history};

const RECENT: usize = 5;

pub fn build() -> TrayIcon {
    let builder = TrayIconBuilder::new().with_menu(Box::new(menu()));
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_templated(icon());
    #[cfg(not(target_os = "macos"))]
    let builder = builder.with_icon(icon());
    builder.build().unwrap()
}

/// The menu lists recent snaps, so it has to be rebuilt whenever history changes.
pub fn refresh(tray: &TrayIcon) {
    tray.set_menu(Some(Box::new(menu())));
}

fn menu() -> Menu {
    let menu = Menu::new();
    menu.append(&MenuItem::with_id(Command::Capture.id(), "Capture", true, None)).unwrap();
    let recent: Vec<_> = history::list().into_iter().take(RECENT).collect();
    if !recent.is_empty() {
        menu.append(&PredefinedMenuItem::separator()).unwrap();
    }
    for entry in recent {
        let label = entry.taken.format("%b %-d, %H:%M").to_string();
        let item = IconMenuItem::with_id(Command::Open(entry.id).id(), label, true, thumbnail(&entry.thumb), None);
        menu.append(&item).unwrap();
    }
    menu.append_items(&[
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(Command::Settings.id(), "Settings…", true, None),
        &MenuItem::with_id(Command::Quit.id(), "Quit", true, None),
    ])
    .unwrap();
    menu
}

/// The latest render, letterboxed to a fixed size so the dates line up. macOS caps menu
/// item icons at 18pt tall; this is 27×18pt at 2x.
fn thumbnail(path: &Path) -> Option<menu::Icon> {
    const W: u32 = 54;
    const H: u32 = 36;
    let image = image::open(path).ok()?.thumbnail(W, H).to_rgba8();
    let mut canvas = image::RgbaImage::new(W, H);
    let (x, y) = ((W - image.width()) / 2, (H - image.height()) / 2);
    image::imageops::overlay(&mut canvas, &image, x.into(), y.into());
    menu::Icon::from_rgba(canvas.into_raw(), W, H).ok()
}

/// The logo at 18pt 2x, rendered by build.rs.
fn icon() -> Icon {
    let rgba = include_bytes!(concat!(env!("OUT_DIR"), "/tray.rgba"));
    Icon::from_rgba(rgba.to_vec(), 36, 36).unwrap()
}
