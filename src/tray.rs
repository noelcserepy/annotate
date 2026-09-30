use tiny_skia::{FillRule, LineCap, Paint, PathBuilder, Pixmap, Stroke, Transform};
use tray_icon::{
    Icon, TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuItem, PredefinedMenuItem},
};

use crate::Command;

pub fn build() -> TrayIcon {
    let menu = Menu::new();
    menu.append_items(&[
        &MenuItem::with_id(Command::Capture.id(), "Capture", true, None),
        &MenuItem::with_id(Command::History.id(), "History", true, None),
        &MenuItem::with_id(Command::Settings.id(), "Settings…", true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(Command::Quit.id(), "Quit", true, None),
    ])
    .unwrap();
    TrayIconBuilder::new().with_menu(Box::new(menu)).with_icon_templated(icon()).build().unwrap()
}

/// A text box with a connector running down to a dot: the app's one move, drawn as an
/// 18pt template image at 2x.
fn icon() -> Icon {
    let mut pixmap = Pixmap::new(36, 36).unwrap();
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = true;

    let mut pb = PathBuilder::new();
    pb.push_rect(tiny_skia::Rect::from_xywh(14., 5., 19., 12.).unwrap());
    pixmap.fill_path(&pb.finish().unwrap(), &paint, FillRule::Winding, Transform::identity(), None);

    let mut pb = PathBuilder::new();
    pb.move_to(20., 17.);
    pb.line_to(20., 20.);
    pb.line_to(10., 30.);
    let stroke = Stroke { width: 2.5, line_cap: LineCap::Round, ..Default::default() };
    pixmap.stroke_path(&pb.finish().unwrap(), &paint, &stroke, Transform::identity(), None);

    let dot = PathBuilder::from_circle(9., 31., 3.).unwrap();
    pixmap.fill_path(&dot, &paint, FillRule::Winding, Transform::identity(), None);

    Icon::from_rgba(pixmap.take(), 36, 36).unwrap()
}
