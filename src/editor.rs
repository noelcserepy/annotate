//! The annotation window: a single composited bitmap plus mouse and keyboard handling.

use std::sync::Arc;

use cosmic_text::{Action, Cursor, Edit, Motion, Selection};
use gpui::{
    App, AppContext, Bounds, ClipboardItem, Context, Corners, CursorStyle, FocusHandle, KeyDownEvent, KeyUpEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderImage, Rgba, Size, TitlebarOptions, Window, WindowBounds,
    WindowOptions, canvas, div, prelude::*, px, size,
};
use tiny_skia::{Color, Pixmap};

use crate::{
    doc::{Arrow, Callout, Dims, Doc, End, P, R, Side, Target, place},
    history::{self, Capture},
    render::{self, Frame, Scene},
};

/// Smallest window content in points, so tiny captures still get a usable window.
const MIN_SIZE: (f32, f32) = (160., 80.);
/// Room in points the window adds past the canvas while it grows, so most keystrokes
/// leave the window alone.
const SLACK: f32 = 120.;
/// Diameter in points of the delete button on a text box's or rect's corner, or an arrow's
/// middle, and of a rect's move handle.
const CLOSE: f32 = 18.;
/// Side in points of a rect's resize handle.
const RESIZE: f32 = 10.;
/// Size in points of the handle beside a text box that moves it, and its distance from the box.
const GRIP: (f32, f32) = (10., 20.);
const GRIP_GAP: f32 = 4.;

enum Mode {
    Idle,
    Selected(usize),
    /// Typing into box `index`; `text` holds its cursor and selection.
    Editing {
        index: usize,
        text: cosmic_text::Editor<'static>,
    },
}

/// Something with a delete button.
#[derive(Clone, Copy, PartialEq)]
enum Item {
    Callout(usize),
    Arrow(usize),
    Rect(usize),
    /// Callout `i`'s rect target. Deleting it deletes the callout.
    Target(usize),
}

/// While its key is held, every drag draws a standalone arrow or rectangle.
#[derive(Clone, Copy, PartialEq)]
enum Tool {
    Arrow,
    Rect,
}

impl Tool {
    fn for_key(key: &str) -> Option<Self> {
        match key {
            "a" => Some(Tool::Arrow),
            "r" => Some(Tool::Rect),
            _ => None,
        }
    }
}

#[derive(Default)]
enum Drag {
    #[default]
    None,
    /// Pressed on the image. A click adds a pointer callout; moving turns it into a marquee.
    New {
        start: P,
    },
    Marquee {
        start: P,
        end: P,
    },
    /// Moving a text box by its handle.
    Box {
        index: usize,
        grab: P,
    },
    /// Moving an arrow end by the pointer's travel since `last`.
    End {
        end: End,
        last: P,
    },
    /// Drawing a standalone arrow with the `Tool`.
    Arrow(usize),
    /// Moving a rect by its handle.
    MoveRect {
        item: Item,
        grab: P,
    },
    /// Spanning a rect from `fixed` to the pointer: resizing by its handle, or drawing a new
    /// one with the `Tool`.
    Resize {
        item: Item,
        fixed: P,
    },
    /// Selecting text inside the box being edited.
    Text,
}

enum Hit {
    Box(usize),
    End(End),
    Image,
    Nothing,
}

pub struct Editor {
    id: String,
    image: Pixmap,
    fill: Color,
    doc: Doc,
    dims: Dims,
    undo: Vec<Doc>,
    mode: Mode,
    drag: Drag,
    frame: Frame,
    shown: Arc<RenderImage>,
    /// The document region the window shows, in image pixels.
    view: R,
    zoom: f32,
    /// What the pointer is on or near, which shows its delete button (and a box's handle).
    hover: Option<Item>,
    /// The arrowhead under the pointer or being dragged, drawn larger.
    hot: Option<End>,
    tool: Option<Tool>,
    focus: FocusHandle,
}

/// Open `capture` in an editor, or bring its editor forward if it has one. Two editors on
/// one entry would each save over the other's work.
pub fn open(capture: Capture, cx: &mut App) {
    cx.activate(true);
    let editors = cx.windows().into_iter().filter_map(|w| w.downcast::<Editor>());
    if let Some(open) = editors.into_iter().find(|w| w.read(cx).is_ok_and(|e| e.id == capture.id)) {
        open.update(cx, |_, window, _| window.activate_window()).ok();
        return;
    }
    let Capture { id, image, doc } = capture;
    let dims = Dims::new(doc.scale);
    let fill = render::edge_color(&image);
    let frame =
        render::compose(Scene { image: &image, fill, doc: &doc, dims: &dims, editing: None, selected: None, marquee: None, hot: None });
    let (view, zoom) = fit_view(frame.bounds, None, doc.scale, cx.primary_display().map(|d| d.bounds().size));
    let content = content_size(view, doc.scale, zoom);

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(content.0), px(content.1)), cx))),
        titlebar: Some(TitlebarOptions { title: None, appears_transparent: false, traffic_light_position: None }),
        is_resizable: false,
        ..Default::default()
    };
    let handle = cx.open_window(options, |window, cx| {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            window.focus(&focus);
            // Quitting drops windows without asking them to close.
            cx.on_app_quit(|editor: &mut Editor, cx| {
                editor.save(cx);
                async {}
            })
            .detach();
            // A key released in another app never reaches us, so drop the held tool.
            cx.observe_window_activation(window, |editor: &mut Editor, window, _| {
                if !window.is_window_active() {
                    editor.tool = None;
                }
            })
            .detach();
            let shown = Arc::new(render::to_render_image(&frame.pixmap));
            Editor {
                id,
                image,
                fill,
                doc,
                dims,
                undo: Vec::new(),
                mode: Mode::Idle,
                drag: Drag::None,
                frame,
                shown,
                view,
                zoom,
                hover: None,
                hot: None,
                tool: None,
                focus,
            }
        })
    });
    if let Ok(handle) = handle {
        handle
            .update(cx, |_, window, cx| {
                let editor = cx.entity();
                window.on_window_should_close(cx, move |_, cx| {
                    editor.update(cx, |editor, cx| editor.save(cx));
                    return_focus(cx);
                    true
                });
            })
            .ok();
    }
}

/// The region to show for `bounds`, and its zoom. While the user types or drags, the view
/// shown now is passed as `grow_from` and only grows, by `SLACK` past the canvas. Otherwise
/// the view hugs the canvas.
fn fit_view(bounds: R, grow_from: Option<R>, scale: f32, screen: Option<Size<Pixels>>) -> (R, f32) {
    let b = bounds;
    let mut v = match grow_from {
        Some(v) => {
            let s = SLACK * scale;
            let x0 = if b.x < v.x { b.x - s } else { v.x };
            let y0 = if b.y < v.y { b.y - s } else { v.y };
            let x1 = if b.right() > v.right() { b.right() + s } else { v.right() };
            let y1 = if b.bottom() > v.bottom() { b.bottom() + s } else { v.bottom() };
            R::new(x0, y0, x1 - x0, y1 - y0)
        }
        None => b,
    };
    let zoom = screen.map_or(1., |screen| {
        let (max_w, max_h) = (f32::from(screen.width) - 80., f32::from(screen.height) - 140.);
        1f32.min(max_w * scale / v.w).min(max_h * scale / v.h)
    });
    let (min_w, min_h) = (MIN_SIZE.0 * scale / zoom, MIN_SIZE.1 * scale / zoom);
    if v.w < min_w {
        v.x -= (min_w - v.w) / 2.;
        v.w = min_w;
    }
    if v.h < min_h {
        v.y -= (min_h - v.h) / 2.;
        v.h = min_h;
    }
    // Whole points, so the window and the canvas in it sit on device pixels.
    let snap = |n: f32, round: fn(f32) -> f32| round(n / scale) * scale;
    let v =
        R::spanning(P::new(snap(v.x, f32::floor), snap(v.y, f32::floor)), P::new(snap(v.right(), f32::ceil), snap(v.bottom(), f32::ceil)));
    (v, zoom)
}

/// Once the closing window is the last one, give focus back to the app the user came from,
/// ready to paste.
fn return_focus(cx: &mut App) {
    if cx.windows().len() <= 1 {
        cx.hide();
    }
}

fn content_size(view: R, scale: f32, zoom: f32) -> (f32, f32) {
    ((view.w / scale * zoom).round(), (view.h / scale * zoom).round())
}

impl Editor {
    fn editing(&self) -> Option<usize> {
        match self.mode {
            Mode::Editing { index, .. } => Some(index),
            _ => None,
        }
    }

    /// The box being edited or selected.
    fn active(&self) -> Option<usize> {
        match self.mode {
            Mode::Selected(i) | Mode::Editing { index: i, .. } => Some(i),
            Mode::Idle => None,
        }
    }

    /// Text selected in the box being edited.
    fn selection(&self) -> Option<String> {
        match &self.mode {
            Mode::Editing { text, .. } => text.copy_selection().filter(|s| !s.is_empty()),
            _ => None,
        }
    }

    fn image_rect(&self) -> R {
        R::new(0., 0., self.image.width() as f32, self.image.height() as f32)
    }

    fn to_doc(&self, position: Point<Pixels>) -> P {
        let k = self.doc.scale / self.zoom;
        P::new(self.view.x + f32::from(position.x) * k, self.view.y + f32::from(position.y) * k)
    }

    /// Position relative to the text inside box `i`.
    fn text_local(&self, i: usize, p: P) -> (i32, i32) {
        let b = self.frame.boxes[i];
        ((p.x - b.x - self.dims.pad_x) as i32, (p.y - b.y - self.dims.pad_y) as i32)
    }

    fn hit(&self, p: P) -> Hit {
        if let Some(i) = self.frame.boxes.iter().rposition(|b| b.contains(p)) {
            return Hit::Box(i);
        }
        // Standalone arrows draw over callout connectors.
        let near = |t: P| t.dist(p) <= self.dims.grab;
        let arrow = self.doc.arrows.iter().enumerate().rev().find_map(|(i, a)| {
            if near(a.to) {
                Some(End::Head(i))
            } else if near(a.from) {
                Some(End::Tail(i))
            } else {
                None
            }
        });
        // A rect target is grabbed by its own handles, not its tip.
        let point = |c: &Callout| matches!(c.target, Target::Point(t) if near(t));
        let end = arrow.or_else(|| self.doc.callouts.iter().rposition(point).map(End::Callout));
        if let Some(end) = end {
            return Hit::End(end);
        }
        if self.image_rect().contains(p) { Hit::Image } else { Hit::Nothing }
    }

    /// The box the pointer is near enough to reach its delete button and handle, else the
    /// standalone arrow or rect outline under it, then a rect target's outline. None mid-drag.
    fn hovered(&self, position: Point<Pixels>) -> Option<Item> {
        if !matches!(self.drag, Drag::None) {
            return None;
        }
        let p = self.to_doc(position);
        let reach = (GRIP_GAP + GRIP.0).max(CLOSE / 2.) * self.doc.scale / self.zoom;
        if let Some(i) = self.frame.boxes.iter().rposition(|b| b.inflate(reach).contains(p)) {
            return Some(Item::Callout(i));
        }
        let near = self.dims.grab;
        let button = CLOSE / 2. * self.doc.scale / self.zoom;
        let on_arrow = |a: &Arrow| p.dist_to_segment(a.from, a.to) <= near || p.dist(self.arrow_delete_spot(a)) <= button;
        if let Some(i) = self.doc.arrows.iter().rposition(on_arrow) {
            return Some(Item::Arrow(i));
        }
        // Wide enough to keep the corner buttons in reach.
        let band = near.max(button);
        let on_outline = |r: &R| r.inflate(band).contains(p) && !r.inflate(-band).contains(p);
        if let Some(i) = self.doc.rects.iter().rposition(on_outline) {
            return Some(Item::Rect(i));
        }
        let on_target = |c: &Callout| matches!(c.target, Target::Rect(r) if on_outline(&r));
        self.doc.callouts.iter().rposition(on_target).map(Item::Target)
    }

    /// The rect `item` names, standalone or a callout's target.
    fn rect(&self, item: Item) -> Option<R> {
        match item {
            Item::Rect(i) => self.doc.rects.get(i).copied(),
            Item::Target(i) => match self.doc.callouts.get(i)?.target {
                Target::Rect(r) => Some(r),
                Target::Point(_) => None,
            },
            Item::Callout(_) | Item::Arrow(_) => None,
        }
    }

    fn rect_mut(&mut self, item: Item) -> Option<&mut R> {
        match item {
            Item::Rect(i) => self.doc.rects.get_mut(i),
            Item::Target(i) => match &mut self.doc.callouts.get_mut(i)?.target {
                Target::Rect(r) => Some(r),
                Target::Point(_) => None,
            },
            Item::Callout(_) | Item::Arrow(_) => None,
        }
    }

    /// Where `item`'s delete button sits, if it still exists. Never on a line, where a click
    /// meant for the line would land on it.
    fn delete_spot(&self, item: Item) -> Option<P> {
        match item {
            Item::Callout(i) => {
                let b = self.frame.boxes.get(i)?;
                // A box corner its connector can't run past.
                Some(match self.doc.callouts.get(i)?.side {
                    Side::Left => P::new(b.x, b.y),
                    Side::Bottom => P::new(b.right(), b.bottom()),
                    Side::Right | Side::Top => P::new(b.right(), b.y),
                })
            }
            Item::Arrow(i) => self.doc.arrows.get(i).map(|a| self.arrow_delete_spot(a)),
            Item::Rect(_) | Item::Target(_) => self.rect(item).map(|r| P::new(r.right(), r.y)),
        }
    }

    /// Beside the arrow's middle, on its upper side, just clear of the line.
    fn arrow_delete_spot(&self, a: &Arrow) -> P {
        let along = a.to - a.from;
        let len = along.x.hypot(along.y).max(0.001);
        let normal = P::new(along.y / len, -along.x / len);
        let normal = if normal.y > 0. || (normal.y == 0. && normal.x < 0.) { P::new(-normal.x, -normal.y) } else { normal };
        let off = CLOSE * self.doc.scale / self.zoom;
        P::new((a.from.x + a.to.x) / 2. + normal.x * off, (a.from.y + a.to.y) / 2. + normal.y * off)
    }

    /// The arrow end a press would grab, or the one being dragged.
    fn hot_end(&self, position: Point<Pixels>) -> Option<End> {
        match self.drag {
            Drag::End { end, .. } => Some(end),
            Drag::None if self.tool.is_none() => match self.hit(self.to_doc(position)) {
                Hit::End(end) => Some(end),
                _ => None,
            },
            _ => None,
        }
    }

    fn checkpoint(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
    }

    /// Stop editing. Empty callouts are removed; returns the removed index.
    fn commit(&mut self) -> Option<usize> {
        let i = self.editing()?;
        self.mode = Mode::Idle;
        if self.doc.callouts[i].text.trim().is_empty() {
            self.doc.callouts.remove(i);
            return Some(i);
        }
        None
    }

    /// Stop editing, and return where `item` is afterwards: None if the commit removed it.
    fn commit_keeping(&mut self, item: Item) -> Option<Item> {
        let removed = self.commit();
        let shift = |i: usize| match removed {
            Some(r) if r == i => None,
            Some(r) if r < i => Some(i - 1),
            _ => Some(i),
        };
        match item {
            Item::Callout(i) => shift(i).map(Item::Callout),
            Item::Target(i) => shift(i).map(Item::Target),
            Item::Arrow(_) | Item::Rect(_) => Some(item),
        }
    }

    fn delete(&mut self, i: usize) {
        self.checkpoint();
        self.doc.callouts.remove(i);
        self.mode = Mode::Idle;
    }

    /// The delete button: drops `item`, finishing any edit first.
    fn delete_clicked(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        match self.commit_keeping(item) {
            Some(Item::Callout(i) | Item::Target(i)) => self.delete(i),
            Some(Item::Arrow(i)) => {
                self.checkpoint();
                self.doc.arrows.remove(i);
            }
            Some(Item::Rect(i)) => {
                self.checkpoint();
                self.doc.rects.remove(i);
            }
            None => {}
        }
        self.mode = Mode::Idle;
        self.refresh(window, cx);
    }

    fn start_editing(&mut self, i: usize, click: Option<(i32, i32)>) {
        let mut text = render::text();
        let buffer = render::text_buffer(&mut text.fonts, &self.doc.callouts[i].text, &self.dims);
        let mut editor = cosmic_text::Editor::new(buffer);
        let action = match click {
            Some((x, y)) => Action::Click { x, y },
            None => Action::Motion(Motion::BufferEnd),
        };
        editor.action(&mut text.fonts, action);
        self.mode = Mode::Editing { index: i, text: editor };
    }

    fn create(&mut self, target: Target) {
        self.checkpoint();
        let size = (self.image.width() as f32, self.image.height() as f32);
        let (side, anchor) = place(&target, size, &self.dims);
        self.doc.callouts.push(Callout { target, side, anchor, text: String::new() });
        self.start_editing(self.doc.callouts.len() - 1, None);
    }

    fn edit(&mut self, f: impl FnOnce(&mut cosmic_text::Editor<'static>, &mut cosmic_text::FontSystem)) {
        let Mode::Editing { index, text: editor } = &mut self.mode else { return };
        let mut text = render::text();
        f(editor, &mut text.fonts);
        editor.shape_as_needed(&mut text.fonts, false);
        self.doc.callouts[*index].text = editor.with_buffer(render::buffer_text);
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live = self.editing().is_some() || !matches!(self.drag, Drag::None);
        self.hot = self.hot_end(window.mouse_position());
        let hot = self.hot;
        let selected = match self.mode {
            Mode::Selected(i) => Some(i),
            _ => None,
        };
        let editing = match &mut self.mode {
            Mode::Editing { index, text } => Some((*index, text)),
            _ => None,
        };
        let marquee = match self.drag {
            Drag::Marquee { start, end } => Some(R::spanning(start, end)),
            _ => None,
        };
        let scene = Scene { image: &self.image, fill: self.fill, doc: &self.doc, dims: &self.dims, editing, selected, marquee, hot };
        self.frame = render::compose(scene);
        self.hover = self.hovered(window.mouse_position());
        let shown = Arc::new(render::to_render_image(&self.frame.pixmap));
        window.drop_image(std::mem::replace(&mut self.shown, shown)).ok();

        let screen = window.display(cx).map(|d| d.bounds().size);
        let (view, zoom) = fit_view(self.frame.bounds, live.then_some(self.view), self.doc.scale, screen);
        if view != self.view || zoom != self.zoom {
            self.reframe(view, zoom, window, cx);
        }
        cx.notify();
    }

    /// Switch to `view`, keeping the screenshot where it is on screen. Returns the shift of
    /// the window's top-left and its new content size, in points.
    fn set_view(&mut self, view: R, zoom: f32, cx: &mut Context<Self>) -> ((f32, f32), (f32, f32)) {
        let k = self.zoom / self.doc.scale;
        let shift = ((view.x - self.view.x) * k, (view.y - self.view.y) * k);
        self.view = view;
        self.zoom = zoom;
        cx.notify();
        (shift, content_size(view, self.doc.scale, zoom))
    }

    /// The window can only change outside this update. Until it does, `self.view` stays
    /// what the window shows, so frames drawn in between keep the screenshot in place.
    #[cfg(target_os = "macos")]
    fn reframe(&mut self, view: R, zoom: f32, window: &mut Window, cx: &mut Context<Self>) {
        let ns = crate::mac::ns_window(window);
        cx.spawn(async move |this, cx| {
            let Ok((shift, content)) = this.update(cx, |editor, cx| editor.set_view(view, zoom, cx)) else { return };
            if let Some(ns) = ns {
                crate::mac::set_frame(&ns, shift, content);
            }
        })
        .detach();
    }

    #[cfg(not(target_os = "macos"))]
    fn reframe(&mut self, view: R, zoom: f32, window: &mut Window, cx: &mut Context<Self>) {
        let (_, (w, h)) = self.set_view(view, zoom, cx);
        window.resize(size(px(w), px(h)));
    }

    /// The document without caret, selection ring or marquee.
    fn clean_frame(&self) -> Frame {
        render::compose(Scene {
            image: &self.image,
            fill: self.fill,
            doc: &self.doc,
            dims: &self.dims,
            editing: None,
            selected: None,
            marquee: None,
            hot: None,
        })
    }

    fn save(&mut self, cx: &mut App) {
        self.commit();
        history::save(&self.id, &self.doc, &self.clean_frame().pixmap);
        crate::history_changed(cx);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.save(cx);
        window.remove_window();
        return_focus(cx);
    }

    fn copy_and_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit();
        let frame = self.clean_frame();
        let png = render::encode_png(&frame.pixmap, self.doc.scale);
        #[cfg(target_os = "macos")]
        crate::mac::copy_image(&png, (frame.bounds.w / self.doc.scale, frame.bounds.h / self.doc.scale));
        #[cfg(not(target_os = "macos"))]
        cx.write_to_clipboard(ClipboardItem::new_image(&gpui::Image::from_bytes(gpui::ImageFormat::Png, png)));
        self.close(window, cx);
    }

    fn undo(&mut self) {
        if let Some(doc) = self.undo.pop() {
            self.doc = doc;
            self.mode = Mode::Idle;
            self.drag = Drag::None;
        }
    }

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.first_mouse {
            return;
        }
        window.focus(&self.focus);
        let p = self.to_doc(event.position);
        if let Some(tool) = self.tool {
            self.commit();
            self.mode = Mode::Idle;
            self.checkpoint();
            self.drag = match tool {
                Tool::Arrow => {
                    self.doc.arrows.push(Arrow { from: p, to: p });
                    Drag::Arrow(self.doc.arrows.len() - 1)
                }
                Tool::Rect => {
                    self.doc.rects.push(R::spanning(p, p));
                    Drag::Resize { item: Item::Rect(self.doc.rects.len() - 1), fixed: p }
                }
            };
            return self.refresh(window, cx);
        }
        let hit = self.hit(p);
        if let (Hit::Box(i), Some(editing)) = (&hit, self.editing())
            && *i == editing
        {
            let (x, y) = self.text_local(editing, p);
            let action = match event.click_count {
                2 => Action::DoubleClick { x, y },
                3.. => Action::TripleClick { x, y },
                _ => Action::Click { x, y },
            };
            self.edit(|editor, fonts| editor.action(fonts, action));
            self.drag = Drag::Text;
            return self.refresh(window, cx);
        }

        // Finishing an edit may delete an empty callout, shifting later indices down.
        let removed = self.commit();
        let fix = |i: usize| i - removed.is_some_and(|r| r < i) as usize;
        self.drag = match hit {
            Hit::Box(i) if removed != Some(i) => {
                // The frame still has the indices from before the commit.
                let click = self.text_local(i, p);
                self.checkpoint();
                self.start_editing(fix(i), Some(click));
                Drag::Text
            }
            Hit::End(end) if removed.is_none_or(|r| end != End::Callout(r)) => {
                self.checkpoint();
                let end = match end {
                    End::Callout(i) => End::Callout(fix(i)),
                    arrow => arrow,
                };
                Drag::End { end, last: p }
            }
            Hit::Image => Drag::New { start: p },
            _ => Drag::None,
        };
        self.refresh(window, cx);
    }

    /// Pressing box `i`'s handle starts moving it. The box being edited stays in editing.
    fn grab_box(&mut self, i: usize, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let p = self.to_doc(event.position);
        let a = self.frame.anchors[i];
        let grab = P::new(p.x - a.x, p.y - a.y);
        let index = if self.editing() == Some(i) {
            i
        } else {
            let Some(Item::Callout(i)) = self.commit_keeping(Item::Callout(i)) else { return };
            self.mode = Mode::Selected(i);
            i
        };
        self.checkpoint();
        self.drag = Drag::Box { index, grab };
        self.refresh(window, cx);
    }

    /// Pressing a rect's move or resize handle. Resizing keeps the top-left corner in place.
    fn grab_rect(&mut self, item: Item, resize: bool, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus);
        let p = self.to_doc(event.position);
        let item = self.commit_keeping(item);
        if let Some((item, r)) = item.and_then(|item| Some((item, self.rect(item)?))) {
            self.checkpoint();
            let corner = P::new(r.x, r.y);
            self.drag = if resize { Drag::Resize { item, fixed: corner } } else { Drag::MoveRect { item, grab: p - corner } };
        }
        self.refresh(window, cx);
    }

    fn mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            if self.hot_end(event.position) != self.hot {
                return self.refresh(window, cx);
            }
            let hover = self.hovered(event.position);
            if hover != self.hover {
                self.hover = hover;
                cx.notify();
            }
            return;
        }
        let p = self.to_doc(event.position);
        let threshold = self.dims.drag_threshold;
        match self.drag {
            Drag::None => return,
            Drag::New { start } => {
                if start.dist(p) < threshold {
                    return;
                }
                self.drag = Drag::Marquee { start, end: p };
            }
            Drag::Marquee { start, .. } => self.drag = Drag::Marquee { start, end: p },
            Drag::Box { index, grab } => self.doc.callouts[index].anchor = P::new(p.x - grab.x, p.y - grab.y),
            Drag::End { end, last } => {
                let by = p - last;
                match end {
                    End::Callout(i) => {
                        if let Target::Point(t) = &mut self.doc.callouts[i].target {
                            *t = *t + by;
                        }
                    }
                    End::Head(i) => self.doc.arrows[i].to = self.doc.arrows[i].to + by,
                    End::Tail(i) => self.doc.arrows[i].from = self.doc.arrows[i].from + by,
                }
                self.drag = Drag::End { end, last: p };
            }
            Drag::Arrow(i) => self.doc.arrows[i].to = p,
            Drag::MoveRect { item, grab } => {
                if let Some(r) = self.rect_mut(item) {
                    (r.x, r.y) = (p.x - grab.x, p.y - grab.y);
                }
            }
            Drag::Resize { item, fixed } => {
                if let Some(r) = self.rect_mut(item) {
                    *r = R::spanning(fixed, p);
                }
            }
            Drag::Text => {
                let Some(i) = self.editing() else { return };
                let (x, y) = self.text_local(i, p);
                self.edit(|editor, fonts| editor.action(fonts, Action::Drag { x, y }));
            }
        }
        self.refresh(window, cx);
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        let threshold = self.dims.drag_threshold;
        match std::mem::take(&mut self.drag) {
            Drag::New { start } => self.create(Target::Point(start)),
            Drag::Marquee { start, end } => {
                let r = R::spanning(start, end).clamp(&self.image_rect());
                if r.w >= 1. && r.h >= 1. {
                    self.create(Target::Rect(r));
                }
            }
            // A click with a tool draws nothing, and a rect can't be shrunk to nothing.
            Drag::Arrow(i) if self.doc.arrows[i].from.dist(self.doc.arrows[i].to) < threshold => self.undo(),
            Drag::Resize { item, .. } if self.rect(item).is_some_and(|r| r.w.hypot(r.h) < threshold) => self.undo(),
            Drag::None => return,
            _ => {}
        }
        self.refresh(window, cx);
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &event.keystroke;
        let m = k.modifiers;
        cx.stop_propagation();
        // A drag holds an index into the doc, which a key could remove from under it.
        if !matches!(self.drag, Drag::None) {
            return;
        }

        if m.platform {
            match k.key.as_str() {
                "c" => {
                    match self.selection() {
                        Some(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
                        None => self.copy_and_close(window, cx),
                    }
                    return;
                }
                "w" => return self.close(window, cx),
                "z" => self.undo(),
                "a" => self.edit(|e, fonts| {
                    e.set_selection(Selection::Normal(Cursor::new(0, 0)));
                    e.action(fonts, Action::Motion(Motion::BufferEnd));
                }),
                "x" => {
                    if let Some(text) = self.selection() {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        self.edit(|e, _| {
                            e.delete_selection();
                        });
                    }
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        self.edit(|e, _| e.insert_string(&text, None));
                    }
                }
                _ => {}
            }
            if !matches!(k.key.as_str(), "left" | "right" | "up" | "down" | "backspace") {
                return self.refresh(window, cx);
            }
        }

        match (k.key.as_str(), &self.mode) {
            ("escape", Mode::Editing { index, .. }) => {
                let i = *index;
                self.mode = if self.commit().is_some() { Mode::Idle } else { Mode::Selected(i) };
            }
            ("escape", Mode::Selected(_)) => self.mode = Mode::Idle,
            ("escape", Mode::Idle) => return self.close(window, cx),
            ("backspace" | "delete", Mode::Selected(i)) => self.delete(*i),
            (_, Mode::Editing { .. }) => self.type_key(event),
            (key, _) if !m.modified() && Tool::for_key(key).is_some() => {
                if event.is_held {
                    return;
                }
                self.tool = Tool::for_key(key);
            }
            (_, Mode::Selected(i)) if k.key_char.is_some() && !m.control => {
                let i = *i;
                self.checkpoint();
                self.start_editing(i, None);
                self.type_key(event);
            }
            _ => return,
        }
        self.refresh(window, cx);
    }

    fn key_up(&mut self, event: &KeyUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool.is_some() && self.tool == Tool::for_key(&event.keystroke.key) {
            self.tool = None;
            self.refresh(window, cx);
        }
    }

    fn type_key(&mut self, event: &KeyDownEvent) {
        let k = &event.keystroke;
        let m = k.modifiers;
        let motion = match k.key.as_str() {
            "left" if m.platform => Some(Motion::Home),
            "right" if m.platform => Some(Motion::End),
            "left" if m.alt => Some(Motion::LeftWord),
            "right" if m.alt => Some(Motion::RightWord),
            "left" => Some(Motion::Left),
            "right" => Some(Motion::Right),
            "up" if m.platform => Some(Motion::BufferStart),
            "down" if m.platform => Some(Motion::BufferEnd),
            "up" => Some(Motion::Up),
            "down" => Some(Motion::Down),
            "home" => Some(Motion::Home),
            "end" => Some(Motion::End),
            _ => None,
        };
        if let Some(motion) = motion {
            return self.edit(|e, fonts| {
                let collapse = e.selection_bounds().filter(|_| !m.shift);
                if m.shift && e.selection() == Selection::None {
                    e.set_selection(Selection::Normal(e.cursor()));
                } else if !m.shift {
                    e.set_selection(Selection::None);
                }
                // Left/right with a selection lands on its edge, like every text field.
                match (collapse, motion) {
                    (Some((start, _)), Motion::Left) => e.set_cursor(start),
                    (Some((_, end)), Motion::Right) => e.set_cursor(end),
                    _ => e.action(fonts, Action::Motion(motion)),
                }
            });
        }
        match k.key.as_str() {
            "backspace" => self.edit(|e, fonts| {
                let reach = if m.platform {
                    Some(Motion::Home)
                } else if m.alt {
                    Some(Motion::LeftWord)
                } else {
                    None
                };
                if let Some(reach) = reach.filter(|_| e.selection() == Selection::None) {
                    e.set_selection(Selection::Normal(e.cursor()));
                    e.action(fonts, Action::Motion(reach));
                }
                e.action(fonts, Action::Backspace);
            }),
            "delete" => self.edit(|e, fonts| e.action(fonts, Action::Delete)),
            "enter" => self.edit(|e, fonts| e.action(fonts, Action::Enter)),
            _ => {
                if m.control || m.platform {
                    return;
                }
                if let Some(chars) = &k.key_char {
                    self.edit(|e, fonts| {
                        for c in chars.chars() {
                            e.action(fonts, Action::Insert(c));
                        }
                    });
                }
            }
        }
    }
}

impl Render for Editor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let k = self.zoom / self.doc.scale;
        let (b, v) = (self.frame.bounds, self.view);
        let fill = Rgba { r: self.fill.red(), g: self.fill.green(), b: self.fill.blue(), a: 1. };
        let shown = self.shown.clone();
        let active = self.active();
        // Items showing their delete button, boxes their handle, and rects their move and
        // resize handles.
        // A held tool owns every press, so it hides them.
        let controls: Vec<Item> = match self.drag {
            _ if self.tool.is_some() => Vec::new(),
            Drag::None => {
                let active = active.map(Item::Callout).filter(|&i| Some(i) != self.hover);
                [self.hover, active].into_iter().flatten().collect()
            }
            Drag::Box { index, .. } => vec![Item::Callout(index)],
            Drag::MoveRect { item, .. } | Drag::Resize { item, .. } => vec![item],
            _ => Vec::new(),
        };
        let rect_handles: Vec<(Item, R)> = controls.iter().filter_map(|&item| Some((item, self.rect(item)?))).collect();
        let moving_rect = matches!(self.drag, Drag::MoveRect { .. });
        let handles: Vec<usize> = controls
            .iter()
            .filter_map(|&item| match item {
                Item::Callout(i) if i < self.frame.boxes.len() => Some(i),
                _ => None,
            })
            .collect();
        let deletes: Vec<(Item, P)> = controls.iter().filter_map(|&item| Some((item, self.delete_spot(item)?))).collect();
        let moving = matches!(self.drag, Drag::Box { .. });
        let tip_cursor = match self.drag {
            Drag::End { .. } => Some(CursorStyle::ClosedHand),
            _ => self.hot.map(|_| CursorStyle::OpenHand),
        };
        let white = Rgba { r: 1., g: 1., b: 1., a: 1. };
        let dark = Rgba { r: 0.11, g: 0.11, b: 0.12, a: 1. };
        // A control of `size` points centered on document point `p`.
        let at = move |p: P, size: f32| {
            div().absolute().left(px((p.x - v.x) * k - size / 2.)).top(px((p.y - v.y) * k - size / 2.)).size(px(size))
        };
        let dot = move || div().size(px(2.)).rounded_full().bg(white);

        div()
            .size_full()
            .relative()
            .bg(fill)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(Self::key_up))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .child(
                div()
                    .absolute()
                    .left(px((b.x - v.x) * k))
                    .top(px((b.y - v.y) * k))
                    .w(px(b.w * k))
                    .h(px(b.h * k))
                    .cursor(tip_cursor.unwrap_or(CursorStyle::Crosshair))
                    .child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                window.paint_image(bounds, Corners::default(), shown, 0, false).ok();
                            },
                        )
                        .size_full(),
                    ),
            )
            .children(handles.into_iter().map(|i| {
                let b = self.frame.boxes[i];
                // Beside the box, on an edge its connector doesn't leave from.
                let x = match self.doc.callouts[i].side {
                    Side::Right => (b.right() - v.x) * k + GRIP_GAP,
                    _ => (b.x - v.x) * k - GRIP_GAP - GRIP.0,
                };
                div()
                    .absolute()
                    .left(px(x))
                    .top(px((b.center().y - v.y) * k - GRIP.1 / 2.))
                    .w(px(GRIP.0))
                    .h(px(GRIP.1))
                    .rounded_full()
                    .bg(dark)
                    .border_1()
                    .border_color(white)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(2.))
                    .children((0..3).map(|_| div().size(px(2.)).rounded_full().bg(white)))
                    .cursor(if moving { CursorStyle::ClosedHand } else { CursorStyle::OpenHand })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.grab_box(i, event, window, cx);
                        }),
                    )
            }))
            .children(rect_handles.into_iter().flat_map(|(item, r)| {
                let mover = at(P::new(r.x, r.y), CLOSE)
                    .rounded_full()
                    .bg(dark)
                    .border_1()
                    .border_color(white)
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(2.))
                    .children((0..2).map(|_| div().flex().gap(px(2.)).child(dot()).child(dot())))
                    .cursor(if moving_rect { CursorStyle::ClosedHand } else { CursorStyle::OpenHand })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.grab_rect(item, false, event, window, cx);
                        }),
                    );
                let resizer = at(P::new(r.right(), r.bottom()), RESIZE)
                    .rounded(px(2.))
                    .bg(white)
                    .border_1()
                    .border_color(dark)
                    .cursor(CursorStyle::ResizeUpLeftDownRight)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.grab_rect(item, true, event, window, cx);
                        }),
                    );
                [mover.into_any_element(), resizer.into_any_element()]
            }))
            .children(deletes.into_iter().map(|(item, spot)| {
                at(spot, CLOSE)
                    .rounded_full()
                    .bg(dark)
                    .border_1()
                    .border_color(white)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(9.))
                    .text_color(white)
                    .cursor_pointer()
                    .child("✕")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.delete_clicked(item, window, cx);
                        }),
                    )
            }))
    }
}
