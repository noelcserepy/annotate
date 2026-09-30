mod doc;
mod editor;
mod history;
mod history_view;
#[cfg(target_os = "macos")]
mod mac;
mod render;
mod settings;
mod tray;

use gpui::{App, AppContext, Application, Bounds, Global, TitlebarOptions, WindowBounds, WindowHandle, WindowOptions, px, size};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState, hotkey::HotKey};
use tray_icon::{TrayIcon, menu::MenuEvent};

use history_view::HistoryView;
use settings::{Settings, SettingsView};

#[derive(Clone, Copy)]
pub enum Command {
    Capture,
    History,
    Settings,
    Quit,
}

impl Command {
    const ALL: [Command; 4] = [Command::Capture, Command::History, Command::Settings, Command::Quit];

    pub fn id(self) -> &'static str {
        match self {
            Command::Capture => "capture",
            Command::History => "history",
            Command::Settings => "settings",
            Command::Quit => "quit",
        }
    }
}

pub struct AppState {
    pub settings: Settings,
    hotkeys: GlobalHotKeyManager,
    hotkey: Option<HotKey>,
    capturing: bool,
    history: Option<WindowHandle<HistoryView>>,
    settings_window: Option<WindowHandle<SettingsView>>,
    _tray: TrayIcon,
}

impl Global for AppState {}

impl AppState {
    pub fn update_settings(cx: &mut App, f: impl FnOnce(&mut Settings)) {
        let state = cx.global_mut::<AppState>();
        let before = state.settings.clone();
        f(&mut state.settings);
        state.settings.save();
        if before.hotkey != state.settings.hotkey {
            state.set_hotkey(true);
        }
        if before.launch_at_login != state.settings.launch_at_login {
            settings::apply_launch_at_login(state.settings.launch_at_login);
        }
    }

    /// Off while the settings window records a new shortcut, so pressing the current
    /// one doesn't start a capture.
    pub fn set_hotkey_enabled(enabled: bool, cx: &mut App) {
        cx.global_mut::<AppState>().set_hotkey(enabled);
    }

    fn set_hotkey(&mut self, enabled: bool) {
        if let Some(old) = self.hotkey.take() {
            self.hotkeys.unregister(old).ok();
        }
        if !enabled {
            return;
        }
        match self.settings.hotkey.parse::<HotKey>() {
            Ok(hotkey) => match self.hotkeys.register(hotkey) {
                Ok(()) => self.hotkey = Some(hotkey),
                Err(e) => eprintln!("register {}: {e}", self.settings.hotkey),
            },
            Err(e) => eprintln!("parse {}: {e}", self.settings.hotkey),
        }
    }
}

fn run(command: Command, cx: &mut App) {
    match command {
        Command::Capture => capture(cx),
        Command::History => open_history(cx),
        Command::Settings => open_settings(cx),
        Command::Quit => cx.quit(),
    }
}

fn capture(cx: &mut App) {
    let state = cx.global_mut::<AppState>();
    if state.capturing {
        return;
    }
    state.capturing = true;
    // macOS only lets us take focus right after the user's key press. Take it now, so
    // focus comes back to us when screencapture exits and the editor opens in front.
    cx.activate(true);
    cx.spawn(async move |cx| {
        let capture = cx.background_executor().spawn(async { history::capture_region() }).await;
        cx.update(|cx| {
            cx.global_mut::<AppState>().capturing = false;
            match capture {
                Some(capture) => {
                    editor::open(capture, cx);
                    history_changed(cx);
                }
                // Hand focus back to the app the user was in.
                None => cx.hide(),
            }
        })
        .ok();
    })
    .detach();
}

fn window_options(width: f32, height: f32, cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(width), px(height)), cx))),
        titlebar: Some(TitlebarOptions { title: None, appears_transparent: false, traffic_light_position: None }),
        ..Default::default()
    }
}

fn open_history(cx: &mut App) {
    cx.activate(true);
    if let Some(handle) = cx.global::<AppState>().history
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let handle = cx.open_window(window_options(880., 620., cx), |_, cx| cx.new(|_| HistoryView::new())).ok();
    cx.global_mut::<AppState>().history = handle;
}

pub fn history_changed(cx: &mut App) {
    if let Some(handle) = cx.global::<AppState>().history {
        handle.update(cx, |view, _, cx| view.reload(cx)).ok();
    }
}

fn open_settings(cx: &mut App) {
    cx.activate(true);
    if let Some(handle) = cx.global::<AppState>().settings_window
        && handle.update(cx, |_, window, _| window.activate_window()).is_ok()
    {
        return;
    }
    let mut options = window_options(300., 116., cx);
    options.is_resizable = false;
    let handle = cx.open_window(options, |window, cx| {
        let view = cx.new(SettingsView::new);
        // Never leave the shortcut disabled because the window closed mid-recording.
        window.on_window_should_close(cx, |_, cx| {
            AppState::set_hotkey_enabled(true, cx);
            true
        });
        view
    });
    cx.global_mut::<AppState>().settings_window = handle.ok();
}

fn main() {
    std::thread::spawn(|| drop(render::text()));

    Application::new().run(|cx| {
        #[cfg(target_os = "macos")]
        mac::hide_dock_icon();

        let (tx, rx) = async_channel::unbounded::<Command>();
        let hotkey_tx = tx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state == HotKeyState::Pressed {
                hotkey_tx.try_send(Command::Capture).ok();
            }
        }));
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if let Some(command) = Command::ALL.into_iter().find(|c| c.id() == event.id.as_ref()) {
                tx.try_send(command).ok();
            }
        }));

        let settings = Settings::load();
        settings::apply_launch_at_login(settings.launch_at_login);
        let mut state = AppState {
            settings,
            hotkeys: GlobalHotKeyManager::new().unwrap(),
            hotkey: None,
            capturing: false,
            history: None,
            settings_window: None,
            _tray: tray::build(),
        };
        state.set_hotkey(true);
        cx.set_global(state);

        cx.spawn(async move |cx| {
            while let Ok(command) = rx.recv().await {
                cx.update(|cx| run(command, cx)).ok();
            }
        })
        .detach();
    });
}
