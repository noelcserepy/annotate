//! Windows pieces GPUI doesn't cover.

use std::cell::RefCell;

use image::RgbaImage;
use tiny_skia::Pixmap;
use windows::{
    Win32::{
        Foundation::{HANDLE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC,
            DeleteObject, EndPaint, FrameRect, GdiFlush, GetDC, GetStockObject, HBITMAP, HBRUSH, HDC, HGDIOBJ, InvalidateRect, PAINTSTRUCT,
            ReleaseDC, SRCCOPY, SelectObject, WHITE_BRUSH,
        },
        System::{
            DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData},
            LibraryLoader::GetModuleHandleW,
            Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
            Ole::CF_DIB,
        },
        UI::{
            Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, IDC_CROSS, LoadCursorW, RegisterClassW,
                SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_SHOW, SetForegroundWindow, ShowWindow,
                WA_INACTIVE, WM_ACTIVATE, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONDOWN,
                WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            },
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

/// Freeze the screen and let the user drag out a region on it. `None` if they cancelled
/// with Escape, a right click, a click without a drag, or by switching away. Runs on the
/// main thread, whose GPUI message loop also delivers the overlay's messages.
pub async fn select_region() -> Option<RgbaImage> {
    let (done, result) = async_channel::bounded(1);
    // On failure `done` is dropped, which ends the wait below.
    unsafe { open_overlay(done) };
    result.recv().await.ok().flatten()
}

thread_local! {
    static OVERLAY: RefCell<Option<Overlay>> = const { RefCell::new(None) };
}

/// A borderless topmost window over every monitor, showing a dimmed copy of the screen
/// with the selection at full brightness. Client coordinates are screenshot pixels: GPUI
/// makes the process per-monitor DPI aware, so nothing gets scaled.
struct Overlay {
    hwnd: HWND,
    size: (i32, i32),
    shot: Dib,
    dim: Dib,
    /// Each paint draws here first, so the selection never flickers.
    back: Dib,
    drag: Option<((i32, i32), (i32, i32))>,
    done: async_channel::Sender<Option<RgbaImage>>,
}

unsafe fn open_overlay(done: async_channel::Sender<Option<RgbaImage>>) -> Option<()> {
    unsafe {
        let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
        let size = (GetSystemMetrics(SM_CXVIRTUALSCREEN), GetSystemMetrics(SM_CYVIRTUALSCREEN));
        let shot = Dib::new(size)?;
        let mut dim = Dib::new(size)?;
        let back = Dib::new(size)?;
        let screen = GetDC(None);
        let copied = BitBlt(shot.dc, 0, 0, size.0, size.1, Some(screen), x, y, SRCCOPY);
        ReleaseDC(None, screen);
        copied.ok()?;
        _ = GdiFlush();
        for (d, s) in dim.pixels_mut().iter_mut().zip(shot.pixels()) {
            *d = (*s as u32 * 3 / 5) as u8;
        }

        let instance = GetModuleHandleW(None).ok()?.into();
        let class = w!("AnnotateCapture");
        // Fails harmlessly once the class exists.
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(overlay_proc),
            hInstance: instance,
            hCursor: LoadCursorW(None, IDC_CROSS).ok()?,
            lpszClassName: class,
            ..Default::default()
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class,
            w!(""),
            WS_POPUP,
            x,
            y,
            size.0,
            size.1,
            None,
            None,
            Some(instance),
            None,
        )
        .ok()?;
        OVERLAY.set(Some(Overlay { hwnd, size, shot, dim, back, drag: None, done }));
        // Showing it delivers messages right away, so the overlay must be in place first.
        // A process that just received its hotkey may take the foreground.
        _ = ShowWindow(hwnd, SW_SHOW);
        _ = SetForegroundWindow(hwnd);
        Some(())
    }
}

/// Destroying the window sends it messages, so the overlay leaves `OVERLAY` before that.
fn close_overlay(result: Option<RgbaImage>) {
    let Some(overlay) = OVERLAY.take() else { return };
    overlay.done.try_send(result).ok();
    unsafe {
        ReleaseCapture().ok();
        DestroyWindow(overlay.hwnd).ok();
    }
}

unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let point = ((lparam.0 & 0xffff) as i16 as i32, ((lparam.0 >> 16) & 0xffff) as i16 as i32);
    match msg {
        WM_LBUTTONDOWN => OVERLAY.with_borrow_mut(|o| {
            if let Some(o) = o {
                o.drag = Some((point, point));
                unsafe { SetCapture(hwnd) };
            }
        }),
        WM_MOUSEMOVE => OVERLAY.with_borrow_mut(|o| {
            if let Some(o) = o
                && let Some((start, end)) = o.drag
            {
                let before = o.selection();
                o.drag = Some((start, point));
                if point != end {
                    for r in [before, o.selection()].into_iter().flatten() {
                        // Take in the border too.
                        let r = inflate(r, 2);
                        _ = unsafe { InvalidateRect(Some(hwnd), Some(&r), false) };
                    }
                }
            }
        }),
        WM_LBUTTONUP => close_overlay(OVERLAY.with_borrow(|o| o.as_ref().and_then(Overlay::crop))),
        WM_RBUTTONDOWN => close_overlay(None),
        WM_KEYDOWN if wparam.0 == VK_ESCAPE.0 as usize => close_overlay(None),
        WM_ACTIVATE if (wparam.0 & 0xffff) as u32 == WA_INACTIVE => close_overlay(None),
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            OVERLAY.with_borrow(|o| o.as_ref().map(|o| o.paint(hdc, ps.rcPaint)));
            _ = EndPaint(hwnd, &ps);
        },
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
    LRESULT(0)
}

impl Overlay {
    /// The dragged rectangle, kept on the screenshot.
    fn selection(&self) -> Option<RECT> {
        let ((x1, y1), (x2, y2)) = self.drag?;
        let (w, h) = self.size;
        Some(RECT {
            left: x1.min(x2).clamp(0, w),
            top: y1.min(y2).clamp(0, h),
            right: x1.max(x2).clamp(0, w),
            bottom: y1.max(y2).clamp(0, h),
        })
    }

    /// `None` for a click without a drag.
    fn crop(&self) -> Option<RgbaImage> {
        let r = self.selection()?;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        if w < 3 || h < 3 {
            return None;
        }
        let rows = self.shot.pixels().chunks(self.size.0 as usize * 4).skip(r.top as usize).take(h as usize);
        // The screenshot is BGRA with no alpha.
        let rgba =
            rows.flat_map(|row| row[r.left as usize * 4..r.right as usize * 4].chunks(4).flat_map(|p| [p[2], p[1], p[0], 255])).collect();
        RgbaImage::from_raw(w as u32, h as u32, rgba)
    }

    fn paint(&self, hdc: HDC, area: RECT) {
        let copy = |to: HDC, from: HDC, r: RECT| unsafe {
            BitBlt(to, r.left, r.top, r.right - r.left, r.bottom - r.top, Some(from), r.left, r.top, SRCCOPY).ok()
        };
        copy(self.back.dc, self.dim.dc, area);
        if let Some(r) = self.selection() {
            copy(self.back.dc, self.shot.dc, r);
            unsafe { FrameRect(self.back.dc, &inflate(r, 1), HBRUSH(GetStockObject(WHITE_BRUSH).0)) };
        }
        copy(hdc, self.back.dc, area);
    }
}

fn inflate(r: RECT, by: i32) -> RECT {
    RECT { left: r.left - by, top: r.top - by, right: r.right + by, bottom: r.bottom + by }
}

/// A top-down 32-bit bitmap selected into its own DC, with its pixels in reach.
struct Dib {
    dc: HDC,
    bitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    len: usize,
}

impl Dib {
    unsafe fn new((w, h): (i32, i32)) -> Option<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                // Negative height means rows run top-down.
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        unsafe {
            let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let dc = CreateCompatibleDC(None);
            let old = SelectObject(dc, bitmap.into());
            Some(Dib { dc, bitmap, old, bits: bits.cast(), len: w as usize * h as usize * 4 })
        }
    }

    fn pixels(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.bits, self.len) }
    }

    fn pixels_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, self.len) }
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            _ = DeleteObject(self.bitmap.into());
            _ = DeleteDC(self.dc);
        }
    }
}
