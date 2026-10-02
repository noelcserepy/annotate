use std::{fs, path::PathBuf};

use gpui::{
    Context, FocusHandle, KeyDownEvent, Keystroke, MouseButton, Window, div, prelude::*, px, rgb,
};
use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// global-hotkey syntax, e.g. "cmd+shift+4".
    pub hotkey: String,
    pub launch_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { hotkey: "cmd+shift+4".into(), launch_at_login: true }
    }
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

/// "cmd+shift+4" -> "⌘⇧4"
fn display(hotkey: &str) -> String {
    let mut mods = String::new();
    let mut key = String::new();
    for token in hotkey.split('+') {
        match token {
            "ctrl" => mods.push('⌃'),
            "alt" => mods.push('⌥'),
            "shift" => mods.push('⇧'),
            "cmd" => mods.push('⌘'),
            k => key = k.to_uppercase(),
        }
    }
    mods + &key
}

fn to_hotkey(k: &Keystroke) -> Option<String> {
    let m = &k.modifiers;
    if !(m.control || m.alt || m.platform) {
        return None;
    }
    let mut parts = Vec::new();
    for (on, name) in [(m.control, "ctrl"), (m.alt, "alt"), (m.shift, "shift"), (m.platform, "cmd")] {
        if on {
            parts.push(name.to_string());
        }
    }
    parts.push(k.key.clone());
    let hotkey = parts.join("+");
    hotkey.parse::<global_hotkey::hotkey::HotKey>().ok().map(|_| hotkey)
}

pub struct SettingsView {
    focus: FocusHandle,
    recording: bool,
}

impl SettingsView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self { focus: cx.focus_handle(), recording: false }
    }

    fn set_recording(&mut self, recording: bool, cx: &mut Context<Self>) {
        self.recording = recording;
        AppState::set_hotkey_enabled(!recording, cx);
        cx.notify();
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.recording {
            return;
        }
        cx.stop_propagation();
        if event.keystroke.key == "escape" {
            self.set_recording(false, cx);
        } else if let Some(hotkey) = to_hotkey(&event.keystroke) {
            AppState::update_settings(cx, |s| s.hotkey = hotkey);
            self.set_recording(false, cx);
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<AppState>().settings.clone();
        let row = || div().flex().items_center().justify_between().h(px(32.));
        let on = settings.launch_at_login;

        div()
            .size_full()
            .bg(rgb(0x1c1c1e))
            .text_color(rgb(0xe5e5e7))
            .text_size(px(13.))
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                row().child("Shortcut").child(
                    div()
                        .id("hotkey")
                        .track_focus(&self.focus)
                        .on_key_down(cx.listener(Self::key_down))
                        .min_w(px(96.))
                        .h(px(26.))
                        .px_3()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .border_1()
                        .border_color(if self.recording { rgb(0xDC2626) } else { rgb(0x3a3a3c) })
                        .bg(rgb(0x2c2c2e))
                        .cursor_pointer()
                        .child(if self.recording { "…".to_string() } else { display(&settings.hotkey) })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                window.focus(&this.focus);
                                this.set_recording(!this.recording, cx);
                            }),
                        ),
                ),
            )
            .child(
                row().child("Open at login").child(
                    div()
                        .id("login")
                        .w(px(36.))
                        .h(px(20.))
                        .rounded_full()
                        .p(px(2.))
                        .flex()
                        .when(on, |d| d.justify_end())
                        .bg(if on { rgb(0xDC2626) } else { rgb(0x3a3a3c) })
                        .cursor_pointer()
                        .child(div().size(px(16.)).rounded_full().bg(rgb(0xffffff)))
                        .on_click(cx.listener(|_, _, _, cx| {
                            AppState::update_settings(cx, |s| s.launch_at_login = !s.launch_at_login);
                            cx.notify();
                        })),
                ),
            )
    }
}

pub fn apply_launch_at_login(enabled: bool) {
    // Only an installed .app can register itself; `cargo run` builds are skipped.
    let Ok(exe) = std::env::current_exe() else { return };
    if !exe.to_string_lossy().contains(".app/Contents/MacOS/") {
        return;
    }
    let launcher = auto_launch::AutoLaunchBuilder::new()
        .set_app_name("Annotate")
        .set_app_path(&exe.to_string_lossy())
        .set_macos_launch_mode(auto_launch::MacOSLaunchMode::SMAppService)
        .build();
    if let Ok(launcher) = launcher {
        let result = if enabled { launcher.enable() } else { launcher.disable() };
        if let Err(e) = result {
            eprintln!("launch at login: {e}");
        }
    }
}
