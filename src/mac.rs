//! macOS pieces GPUI doesn't cover.

use gpui::Window;
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBitmapImageRep, NSEventModifierFlags, NSPasteboard, NSPasteboardTypePNG,
    NSPasteboardTypeTIFF, NSView, NSWindow,
};
use objc2_foundation::{NSData, NSPoint, NSRect, NSSize};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// GPUI forces a regular (dock) activation policy at launch; we're a menu bar app.
pub fn hide_dock_icon() {
    let mtm = MainThreadMarker::new().unwrap();
    NSApplication::sharedApplication(mtm).setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

/// The key event being handled, as the global-hotkey name of the key's position and whether
/// shift is held.
pub fn current_key() -> Option<(&'static str, bool)> {
    let event = NSApplication::sharedApplication(MainThreadMarker::new()?).currentEvent()?;
    // Key positions by kVK_ANSI_* code from 0x00, named as on a US keyboard. "_" leaves the key to GPUI.
    const ANSI: &str = "asdfhgzxcv_bqweryt123465=97-80]ou[ip_lj'k;\\,/nm.__`";
    let code = event.keyCode() as usize;
    let key = ANSI.get(code..code + 1).filter(|k| *k != "_")?;
    Some((key, event.modifierFlags().contains(NSEventModifierFlags::Shift)))
}

pub fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw() else {
        return None;
    };
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    view.window()
}

/// Resize the content area to `size` points and shift its top-left by `shift`, so the
/// screenshot stays put on screen while the canvas grows in any direction. GPUI's own
/// `resize` keeps the bottom-left corner fixed instead. Changing the frame re-enters
/// GPUI, so call this outside any GPUI update.
pub fn set_frame(ns: &NSWindow, shift: (f32, f32), size: (f32, f32)) {
    let content = ns.contentRectForFrameRect(ns.frame());
    let top = content.origin.y + content.size.height;
    let (w, h) = (size.0 as f64, size.1 as f64);
    let content = NSRect::new(NSPoint::new(content.origin.x + shift.0 as f64, top - shift.1 as f64 - h), NSSize::new(w, h));
    let mut frame = ns.frameRectForContentRect(content);
    if let Some(screen) = ns.screen() {
        let v = screen.visibleFrame();
        frame.origin.x = frame.origin.x.min(v.origin.x + v.size.width - frame.size.width).max(v.origin.x);
        frame.origin.y = frame.origin.y.max(v.origin.y).min(v.origin.y + v.size.height - frame.size.height);
    }
    ns.setFrame_display(frame, true);
}

/// Put the PNG on the pasteboard, plus a TIFF sized in points for apps that ignore
/// PNG DPI and would otherwise paste retina captures at double size.
pub fn copy_image(png: &[u8], points: (f32, f32)) {
    let data = NSData::with_bytes(png);
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    unsafe {
        pasteboard.setData_forType(Some(&data), NSPasteboardTypePNG);
        if let Some(rep) = NSBitmapImageRep::imageRepWithData(&data) {
            rep.setSize(NSSize::new(points.0 as f64, points.1 as f64));
            if let Some(tiff) = rep.TIFFRepresentation() {
                pasteboard.setData_forType(Some(&tiff), NSPasteboardTypeTIFF);
            }
        }
    }
}
