use std::{fs, path::PathBuf, sync::Arc};

use gpui::{
    AnyElement, Bounds, Context, Corner, Corners, Div, FocusHandle, KeyDownEvent, Keystroke, MouseButton, PathPromptOptions, RenderImage,
    Window, anchored, canvas, deferred, div, point, prelude::*, px, rgb, size,
};
use serde::{Deserialize, Serialize};
use tiny_skia::{Paint, Pixmap, Rect, Transform};

use crate::{
    AppState,
    doc::{Arrow, Callout, Dims, Doc, P, R, Side, Style, Target},
    render::{self, Scene},
};

/// The editor's command key, in global-hotkey syntax.
pub const MOD: &str = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub keys: Keys,
    /// The look of new captures.
    pub style: Style,
    /// Where "Save and close" puts images.
    pub save_dir: PathBuf,
    pub launch_at_login: bool,
    /// The welcome window has been shown.
    pub welcomed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { keys: Keys::default(), style: Style::default(), save_dir: default_save_dir(), launch_at_login: true, welcomed: false }
    }
}

fn default_save_dir() -> PathBuf {
    dirs::download_dir().or_else(dirs::home_dir).unwrap_or_default()
}

/// Key combos in global-hotkey syntax, e.g. "cmd+shift+4".
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keys {
    /// Starts a capture from any app.
    pub capture: String,
    pub copy: String,
    pub save: String,
    pub undo: String,
    pub close: String,
    /// Held while dragging, so a single key without modifiers.
    pub arrow: String,
    pub rect: String,
}

impl Default for Keys {
    fn default() -> Self {
        // Ctrl on Windows, which keeps Win+Shift+digit for itself.
        Self {
            capture: format!("{MOD}+shift+4"),
            copy: format!("{MOD}+c"),
            save: format!("{MOD}+s"),
            undo: format!("{MOD}+z"),
            close: "escape".into(),
            arrow: "a".into(),
            rect: "r".into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Shortcut {
    Capture,
    Copy,
    Save,
    Undo,
    Close,
    Arrow,
    Rect,
}

impl Shortcut {
    const ALL: [Shortcut; 7] =
        [Shortcut::Capture, Shortcut::Copy, Shortcut::Save, Shortcut::Undo, Shortcut::Close, Shortcut::Arrow, Shortcut::Rect];

    fn label(self) -> &'static str {
        match self {
            Shortcut::Capture => "Capture (anywhere)",
            Shortcut::Copy => "Copy and close",
            Shortcut::Save => "Save and close",
            Shortcut::Undo => "Undo",
            Shortcut::Close => "Close",
            Shortcut::Arrow => "Arrow (hold and drag)",
            Shortcut::Rect => "Rectangle (hold and drag)",
        }
    }

    pub fn of(self, keys: &Keys) -> &str {
        match self {
            Shortcut::Capture => &keys.capture,
            Shortcut::Copy => &keys.copy,
            Shortcut::Save => &keys.save,
            Shortcut::Undo => &keys.undo,
            Shortcut::Close => &keys.close,
            Shortcut::Arrow => &keys.arrow,
            Shortcut::Rect => &keys.rect,
        }
    }

    fn of_mut(self, keys: &mut Keys) -> &mut String {
        match self {
            Shortcut::Capture => &mut keys.capture,
            Shortcut::Copy => &mut keys.copy,
            Shortcut::Save => &mut keys.save,
            Shortcut::Undo => &mut keys.undo,
            Shortcut::Close => &mut keys.close,
            Shortcut::Arrow => &mut keys.arrow,
            Shortcut::Rect => &mut keys.rect,
        }
    }

    /// The combo `k` sets this shortcut to, if it can be one.
    fn accept(self, k: &Keystroke) -> Option<String> {
        let m = &k.modifiers;
        let keys = if self == Shortcut::Capture { capture_combo(k) } else { combo(k) };
        let ok = match self {
            // A global shortcut without a modifier would swallow that key in every app.
            Shortcut::Capture => (m.control || m.alt || m.platform) && keys.parse::<global_hotkey::hotkey::HotKey>().is_ok(),
            Shortcut::Arrow | Shortcut::Rect => !m.modified() && k.key.chars().count() == 1,
            Shortcut::Copy | Shortcut::Save | Shortcut::Undo | Shortcut::Close => true,
        };
        ok.then_some(keys)
    }
}

/// `k` in global-hotkey syntax.
pub fn combo(k: &Keystroke) -> String {
    let m = &k.modifiers;
    let mods = [(m.control, "ctrl"), (m.alt, "alt"), (m.shift, "shift"), (m.platform, "cmd")];
    let mut parts: Vec<&str> = mods.into_iter().filter(|(on, _)| *on).map(|(_, name)| name).collect();
    // "+" separates the parts, so the key itself is spelled out.
    parts.push(if k.key == "+" { "plus" } else { &k.key });
    parts.join("+")
}

/// global-hotkey registers the key at a position on the keyboard. GPUI's macOS key is the
/// character instead, with shift folded in, so cmd+shift+4 arrives as cmd+$.
#[cfg(target_os = "macos")]
fn capture_combo(k: &Keystroke) -> String {
    let Some((key, shift)) = crate::mac::current_key() else { return combo(k) };
    let modifiers = gpui::Modifiers { shift, ..k.modifiers };
    combo(&Keystroke { modifiers, key: key.into(), key_char: None })
}

#[cfg(not(target_os = "macos"))]
fn capture_combo(k: &Keystroke) -> String {
    combo(k)
}

fn path() -> PathBuf {
    dirs::config_dir().unwrap().join("Annotate").join("settings.json")
}

impl Settings {
    pub fn load() -> Self {
        fs::read(path()).ok().and_then(|json| serde_json::from_slice(&json).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        fs::create_dir_all(path().parent().unwrap()).ok();
        fs::write(path(), serde_json::to_vec_pretty(self).unwrap()).ok();
    }
}

/// "escape" -> "Esc", "z" -> "Z"
fn key_name(key: &str) -> String {
    match key {
        "escape" => "Esc".into(),
        "plus" => "+".into(),
        k if k.chars().count() == 1 => k.to_uppercase(),
        k => k[..1].to_uppercase() + &k[1..],
    }
}

/// "cmd+shift+4" -> "⌘⇧4"
#[cfg(target_os = "macos")]
pub fn display(keys: &str) -> String {
    let mut mods = String::new();
    let mut key = String::new();
    for token in keys.split('+') {
        match token {
            "ctrl" => mods.push('⌃'),
            "alt" => mods.push('⌥'),
            "shift" => mods.push('⇧'),
            "cmd" => mods.push('⌘'),
            k => key = key_name(k),
        }
    }
    mods + &key
}

/// "ctrl+shift+4" -> "Ctrl+Shift+4"
#[cfg(not(target_os = "macos"))]
pub fn display(keys: &str) -> String {
    let names: Vec<String> = keys
        .split('+')
        .map(|token| match token {
            "ctrl" => "Ctrl".into(),
            "alt" => "Alt".into(),
            "shift" => "Shift".into(),
            "cmd" => "Win".into(),
            k => key_name(k),
        })
        .collect();
    names.join("+")
}

const BG: u32 = 0x1c1c1e;
const FIELD: u32 = 0x2c2c2e;
const LINE: u32 = 0x3a3a3c;
const MUTED: u32 = 0x98989d;
const RED: u32 = 0xDC2626;
/// Behind the preview's sample page.
const BACKDROP: u32 = 0xe9e9ec;

const ACCENTS: [[u8; 3]; 7] = [
    [0xDC, 0x26, 0x26],
    [0xF5, 0x9E, 0x0B],
    [0x16, 0xA3, 0x4A],
    [0x25, 0x63, 0xEB],
    [0x93, 0x33, 0xEA],
    [0xDB, 0x27, 0x77],
    [0x11, 0x11, 0x11],
];
const INKS: [[u8; 3]; 3] = [[0xFF, 0xFF, 0xFF], [0x11, 0x11, 0x11], [0xFD, 0xE0, 0x47]];

fn hex([r, g, b]: [u8; 3]) -> u32 {
    (r as u32) << 16 | (g as u32) << 8 | b as u32
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Appearance,
    Shortcuts,
    General,
}

pub struct SettingsView {
    focus: FocusHandle,
    tab: Tab,
    recording: Option<Shortcut>,
    /// The font list is open, filtered by this search.
    font_search: Option<String>,
    families: Vec<String>,
    preview: Option<Preview>,
}

/// The preview image and the style and scale it was drawn at.
struct Preview {
    style: Style,
    scale: f32,
    image: Arc<RenderImage>,
    /// In points.
    size: (f32, f32),
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        Self { focus, tab: Tab::Appearance, recording: None, font_search: None, families: render::families(), preview: None }
    }

    fn set_recording(&mut self, recording: Option<Shortcut>, cx: &mut Context<Self>) {
        self.recording = recording;
        AppState::set_hotkey_enabled(recording.is_none(), cx);
        cx.notify();
    }

    fn fonts(&self) -> Vec<&str> {
        let search = self.font_search.as_deref().unwrap_or_default().to_lowercase();
        self.families.iter().filter(|f| f.to_lowercase().contains(&search)).map(String::as_str).collect()
    }

    fn pick_font(&mut self, font: String, cx: &mut Context<Self>) {
        AppState::update_settings(cx, |s| s.style.font = font);
        self.font_search = None;
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let k = &event.keystroke;
        if let Some(shortcut) = self.recording {
            cx.stop_propagation();
            if let Some(keys) = shortcut.accept(k) {
                AppState::update_settings(cx, |s| *shortcut.of_mut(&mut s.keys) = keys);
                self.set_recording(None, cx);
            }
            return;
        }
        let Some(search) = &mut self.font_search else { return };
        cx.stop_propagation();
        match k.key.as_str() {
            "escape" => self.font_search = None,
            "backspace" => _ = search.pop(),
            "enter" => {
                if let Some(font) = self.fonts().first().map(|f| f.to_string()) {
                    self.pick_font(font, cx);
                }
            }
            "space" => search.push(' '),
            _ if !(k.modifiers.control || k.modifiers.platform) => search.push_str(k.key_char.as_deref().unwrap_or_default()),
            _ => {}
        }
        cx.notify();
    }

    fn choose_folder(&mut self, cx: &mut Context<Self>) {
        let options = PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Choose".into()) };
        let paths = cx.prompt_for_paths(options);
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = paths.await
                && let Some(dir) = paths.pop()
            {
                this.update(cx, |_, cx| {
                    AppState::update_settings(cx, |s| s.save_dir = dir);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn appearance(&mut self, settings: &Settings, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let style = &settings.style;
        div()
            .flex()
            .flex_col()
            .child(row("Box and line colour").child(swatches(&ACCENTS, style.accent, |s, c| s.style.accent = c, cx)))
            .child(row("Text colour").child(swatches(&INKS, style.ink, |s, c| s.style.ink = c, cx)))
            .child(row("Font").child(self.font_picker(&style.font, cx)))
            .child(row("Text size").child(stepper(style.text_size, 1., (10., 32.), |s| &mut s.text_size, cx)))
            .child(row("Line width").child(stepper(style.line_width, 0.5, (1., 8.), |s| &mut s.line_width, cx)))
            .child(self.preview(style, window))
            .into_any_element()
    }

    fn font_picker(&self, current: &str, cx: &mut Context<Self>) -> Div {
        let open = self.font_search.is_some();
        let button = field()
            .w(px(200.))
            .justify_between()
            .when(open, |d| d.border_color(rgb(RED)))
            .child(div().truncate().child(current.to_string()))
            .child("▾")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.font_search = if this.font_search.is_some() { None } else { Some(String::new()) };
                    cx.notify();
                }),
            );
        let list = self.font_search.as_ref().map(|search| {
            let fonts = self.fonts().into_iter().map(|font| {
                let font = font.to_string();
                div()
                    .px_2()
                    .h(px(24.))
                    .flex()
                    .items_center()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(LINE)))
                    .when(font == current, |d| d.text_color(rgb(0xffffff)).font_weight(gpui::FontWeight::SEMIBOLD))
                    .child(font.clone())
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.pick_font(font.clone(), cx)))
            });
            let prompt = match search.as_str() {
                "" => div().text_color(rgb(MUTED)).child(format!("Type to search {} fonts", self.families.len())),
                search => div().child(format!("{search}|")),
            };
            div()
                .occlude()
                .w(px(240.))
                .mt_1()
                .p_1()
                .rounded_md()
                .bg(rgb(FIELD))
                .border_1()
                .border_color(rgb(LINE))
                .shadow_lg()
                .flex()
                .flex_col()
                .child(div().h(px(26.)).px_2().mb_1().flex().items_center().border_b_1().border_color(rgb(LINE)).child(prompt))
                .child(div().id("fonts").max_h(px(220.)).overflow_y_scroll().flex().flex_col().children(fonts))
        });
        // Deferred, so it draws over the rows below.
        div().relative().child(button).when_some(list, |d, list| {
            d.child(div().absolute().top(px(26.)).right_0().child(deferred(anchored().anchor(Corner::TopRight).child(list))))
        })
    }

    fn preview(&mut self, style: &Style, window: &mut Window) -> Div {
        let scale = window.scale_factor();
        if self.preview.as_ref().is_none_or(|p| p.style != *style || p.scale != scale) {
            let pixmap = sample(style, scale);
            let image = Arc::new(render::to_render_image(&pixmap));
            let size = (pixmap.width() as f32 / scale, pixmap.height() as f32 / scale);
            if let Some(old) = self.preview.replace(Preview { style: style.clone(), scale, image, size }) {
                window.drop_image(old.image).ok();
            }
        }
        let preview = self.preview.as_ref().unwrap();
        let (image, (w, h)) = (preview.image.clone(), preview.size);
        div().mt_3().h(px(190.)).rounded_lg().overflow_hidden().bg(rgb(BACKDROP)).child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    // Shrink to fit when large text makes the sample wider than the frame.
                    let k = (f32::from(bounds.size.width) / w).min(f32::from(bounds.size.height) / h).min(1.);
                    let fit = size(px(w * k), px(h * k));
                    let origin = bounds.center() - point(fit.width / 2., fit.height / 2.);
                    window.paint_image(Bounds { origin, size: fit }, Corners::default(), image, 0, false).ok();
                },
            )
            .size_full(),
        )
    }

    fn shortcuts(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .children(Shortcut::ALL.into_iter().map(|shortcut| {
                let recording = self.recording == Some(shortcut);
                row(shortcut.label()).child(
                    field()
                        .min_w(px(96.))
                        .justify_center()
                        .when(recording, |d| d.border_color(rgb(RED)))
                        .child(if recording { "Press keys…".to_string() } else { display(shortcut.of(&settings.keys)) })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                window.focus(&this.focus);
                                this.set_recording((this.recording != Some(shortcut)).then_some(shortcut), cx);
                            }),
                        ),
                )
            }))
            .child(div().mt_2().text_color(rgb(MUTED)).child("Click a shortcut, then press the new keys. Click it again to cancel."))
            .into_any_element()
    }

    fn general(&self, settings: &Settings, cx: &mut Context<Self>) -> AnyElement {
        let on = settings.launch_at_login;
        div()
            .flex()
            .flex_col()
            .child(
                row("Save folder").child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().max_w(px(200.)).truncate().text_color(rgb(MUTED)).child(shown_path(&settings.save_dir)))
                        .child(button("Choose…").on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.choose_folder(cx)))),
                ),
            )
            .child(
                row("Start app on startup").child(
                    div()
                        .w(px(36.))
                        .h(px(20.))
                        .rounded_full()
                        .p(px(2.))
                        .flex()
                        .when(on, |d| d.justify_end())
                        .bg(if on { rgb(RED) } else { rgb(LINE) })
                        .cursor_pointer()
                        .child(div().size(px(16.)).rounded_full().bg(rgb(0xffffff)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| {
                                AppState::update_settings(cx, |s| s.launch_at_login = !s.launch_at_login);
                                cx.notify();
                            }),
                        ),
                ),
            )
            .into_any_element()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<AppState>().settings.clone();
        let page = match self.tab {
            Tab::Appearance => self.appearance(&settings, window, cx),
            Tab::Shortcuts => self.shortcuts(&settings, cx),
            Tab::General => self.general(&settings, cx),
        };
        let tabs = [(Tab::Appearance, "Appearance"), (Tab::Shortcuts, "Shortcuts"), (Tab::General, "General")].map(|(tab, label)| {
            div()
                .px_3()
                .h(px(26.))
                .flex()
                .items_center()
                .rounded_md()
                .cursor_pointer()
                .map(|d| if self.tab == tab { d.bg(rgb(FIELD)) } else { d.text_color(rgb(MUTED)) })
                .child(label)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.tab = tab;
                        this.font_search = None;
                        this.set_recording(None, cx);
                    }),
                )
        });

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(0xe5e5e7))
            .text_size(px(13.))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .child(div().flex().gap_1().px_4().py_2().border_b_1().border_color(rgb(LINE)).children(tabs))
            .child(div().flex_1().px_4().py_2().child(page))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .px_4()
                    .pb_4()
                    .child(button("Reset to defaults").on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            AppState::update_settings(cx, |s| {
                                s.keys = Keys::default();
                                s.style = Style::default();
                                s.save_dir = default_save_dir();
                            });
                            this.set_recording(None, cx);
                        }),
                    ))
                    .child(button("Done").bg(rgb(RED)).border_color(rgb(RED)).text_color(rgb(0xffffff)).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.set_recording(None, cx);
                            window.remove_window();
                        }),
                    )),
            )
    }
}

/// `path` with the home folder as "~" on the Mac, where people know that shorthand.
fn shown_path(path: &std::path::Path) -> String {
    let home = dirs::home_dir().filter(|_| cfg!(target_os = "macos"));
    match home.and_then(|home| path.strip_prefix(home).ok().map(std::path::Path::to_path_buf)) {
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

fn row(label: &'static str) -> Div {
    div().flex().items_center().justify_between().h(px(34.)).child(label)
}

fn field() -> Div {
    div().h(px(26.)).px_3().flex().items_center().rounded_md().bg(rgb(FIELD)).border_1().border_color(rgb(LINE)).cursor_pointer()
}

fn button(label: &'static str) -> Div {
    field().child(label)
}

fn swatches(colors: &[[u8; 3]], current: [u8; 3], set: fn(&mut Settings, [u8; 3]), cx: &mut Context<SettingsView>) -> Div {
    div().flex().gap_1().children(colors.iter().map(|&c| {
        // A ring with a gap, so it shows around every colour, white included.
        div()
            .p(px(2.))
            .rounded_full()
            .border_2()
            .border_color(if c == current { rgb(0xffffff) } else { rgb(BG) })
            .cursor_pointer()
            .child(div().size(px(16.)).rounded_full().bg(rgb(hex(c))).border_1().border_color(rgb(LINE)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_, _, _, cx| {
                    AppState::update_settings(cx, |s| set(s, c));
                    cx.notify();
                }),
            )
    }))
}

fn stepper(value: f32, step: f32, range: (f32, f32), get: fn(&mut Style) -> &mut f32, cx: &mut Context<SettingsView>) -> Div {
    let nudge = |label: &'static str, by: f32| {
        field().w(px(26.)).px_0().justify_center().child(label).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |_, _, _, cx| {
                AppState::update_settings(cx, |s| {
                    let v = get(&mut s.style);
                    *v = (*v + by).clamp(range.0, range.1);
                });
                cx.notify();
            }),
        )
    };
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(nudge("−", -step))
        .child(div().w(px(36.)).flex().justify_center().child(format!("{value}")))
        .child(nudge("+", step))
}

/// A sample capture in `style`: a page with one line called out and an arrow at a picture.
fn sample(style: &Style, scale: f32) -> Pixmap {
    let s = scale;
    let (w, h) = (210., 150.);
    let mut page = Pixmap::new((w * s) as u32, (h * s) as u32).unwrap();
    let mut block = |x: f32, y: f32, bw: f32, bh: f32, color: u32| {
        let mut paint = Paint::default();
        paint.set_color_rgba8((color >> 16) as u8, (color >> 8) as u8, color as u8, 255);
        if let Some(r) = Rect::from_xywh(x * s, y * s, bw * s, bh * s) {
            page.fill_rect(r, &paint, Transform::identity(), None);
        }
    };
    block(0., 0., w, h, BACKDROP);
    block(10., 10., 190., 130., 0xffffff);
    block(24., 24., 90., 9., 0xa1a1aa);
    block(24., 48., 120., 6., 0xd4d4d8);
    block(24., 62., 150., 6., 0xd4d4d8);
    block(24., 76., 105., 6., 0xd4d4d8);
    block(24., 94., 70., 32., 0xe4e4e7);
    block(102., 94., 84., 32., 0xe4e4e7);

    let d = Dims::new(s, style);
    let target = R::new(18. * s, 43. * s, 132. * s, 16. * s);
    let anchor = P::new(w * s + d.gap, target.center().y);
    let doc = Doc {
        scale: s,
        callouts: vec![Callout { target: Target::Rect(target), side: Side::Right, anchor, text: "Typo here".into() }],
        arrows: vec![Arrow { from: P::new(236. * s, 158. * s), to: P::new(180. * s, 118. * s) }],
        rects: Vec::new(),
        style: style.clone(),
    };
    let fill = render::edge_color(&page);
    render::compose(Scene { image: &page, fill, doc: &doc, dims: &d, editing: None, selected: None, marquee: None, hot: None }).pixmap
}

pub fn apply_launch_at_login(enabled: bool) {
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.to_string_lossy();
    let mut builder = auto_launch::AutoLaunchBuilder::new();
    builder.set_app_name("Annotate");
    #[cfg(target_os = "macos")]
    {
        // Only an installed .app can register itself; `cargo run` builds are skipped.
        if !exe.contains(".app/Contents/MacOS/") {
            return;
        }
        builder.set_app_path(&exe).set_macos_launch_mode(auto_launch::MacOSLaunchMode::SMAppService);
    }
    // Registers wherever the exe runs from now, so moving it and starting it again updates the entry.
    // The registry value is a command line, so a path with spaces needs quotes.
    #[cfg(windows)]
    builder.set_app_path(&format!("\"{exe}\"")).set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser);
    if let Ok(launcher) = builder.build() {
        let result = if enabled { launcher.enable() } else { launcher.disable() };
        if let Err(e) = result {
            eprintln!("start on startup: {e}");
        }
    }
}
