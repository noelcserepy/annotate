//! Windows pieces GPUI doesn't cover.

use tiny_skia::Pixmap;
use windows::{
    Win32::{
        Foundation::HANDLE,
        Graphics::Gdi::{BI_RGB, BITMAPINFOHEADER},
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData},
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_DIB,
        },
    },
    core::w,
};

/// Put the PNG on the clipboard for apps that read it (browsers, Office), plus a DIB for
/// the rest. GPUI only writes the PNG, which Paint and most native apps can't paste.
pub fn copy_image(png: &[u8], pixmap: &Pixmap) {
    let header = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: pixmap.width() as i32,
        // Positive height means rows run bottom-up.
        biHeight: pixmap.height() as i32,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let mut dib = unsafe { std::slice::from_raw_parts((&raw const header).cast::<u8>(), size_of::<BITMAPINFOHEADER>()) }.to_vec();
    // The canvas is opaque, so premultiplied RGBA is plain RGBA; DIBs want BGRA.
    for row in pixmap.data().chunks(pixmap.width() as usize * 4).rev() {
        dib.extend(row.chunks(4).flat_map(|p| [p[2], p[1], p[0], p[3]]));
    }
    unsafe {
        if OpenClipboard(None).is_err() {
            return;
        }
        EmptyClipboard().ok();
        set(RegisterClipboardFormatW(w!("PNG")), png);
        set(CF_DIB.0 as u32, &dib);
        CloseClipboard().ok();
    }
}

/// The clipboard owns the memory once SetClipboardData succeeds.
unsafe fn set(format: u32, bytes: &[u8]) {
    let Ok(memory) = (unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }) else { return };
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), GlobalLock(memory).cast(), bytes.len());
        GlobalUnlock(memory).ok();
        SetClipboardData(format, Some(HANDLE(memory.0))).ok();
    }
}
