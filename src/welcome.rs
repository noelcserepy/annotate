//! Shown once, on the first launch: where the app lives and what the keys do.

use gpui::{App, AppContext, Context, MouseButton, Window, div, prelude::*, px, rgb, size};

use crate::{AppState, settings::display};

#[cfg(target_os = "macos")]
const HOME: &str = "Annotate lives in the menu bar.";
#[cfg(not(target_os = "macos"))]
const HOME: &str = "Annotate lives in the system tray at the right end of the taskbar. If you don't see its icon, click the ^ arrow there.";

pub fn open(cx: &mut App) {
    cx.activate(true);
    let mut options = crate::window_options(340., 420., cx);
    options.is_resizable = false;
    cx.open_window(options, |window, cx| {
        // Windows opens new windows behind the app in front.
        window.activate_window();
        cx.new(|_| Welcome)
    })
    .ok();
}

pub struct Welcome;

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let k = &cx.global::<AppState>().settings.keys;
        let keys = [
            ("Capture", display(&k.capture)),
            ("Callout", "Click or drag".into()),
            ("Arrow", format!("Hold {} and drag", display(&k.arrow))),
            ("Rectangle", format!("Hold {} and drag", display(&k.rect))),
            ("Undo", display(&k.undo)),
            ("Copy and close", display(&k.copy)),
            ("Save and close", display(&k.save)),
            ("Close", display(&k.close)),
        ];
        let button = || div().px_4().h(px(26.)).flex().items_center().rounded_md().cursor_pointer();
        let muted = rgb(0x98989d);

        div()
            .size_full()
            .bg(rgb(0x1c1c1e))
            .text_color(rgb(0xe5e5e7))
            .text_size(px(13.))
            // The text wraps differently per OS, so the window takes the height of its content.
            .on_children_prepainted(|children, window, _| {
                let viewport = window.viewport_size();
                if (children[0].size.height - viewport.height).abs() > px(1.) {
                    window.resize(size(viewport.width, children[0].size.height));
                }
            })
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_size(px(15.)).font_weight(gpui::FontWeight::SEMIBOLD).child("Annotate is running"))
                    .child(div().text_color(muted).child(HOME))
                    .child(
                        div().flex().flex_col().gap_1().children(
                            keys.into_iter().map(|(action, key)| {
                                div().flex().justify_between().child(action).child(div().text_color(muted).child(key))
                            }),
                        ),
                    )
                    .child(
                        div()
                            .text_color(muted)
                            .child("Its menu has your recent captures. Customize changes colours, font, shortcuts and the save folder."),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(button().bg(rgb(0x2c2c2e)).border_1().border_color(rgb(0x3a3a3c)).child("Customize…").on_mouse_down(
                                MouseButton::Left,
                                |_, window, cx| {
                                    crate::open_settings(cx);
                                    window.remove_window();
                                },
                            ))
                            .child(
                                button()
                                    .bg(rgb(0xDC2626))
                                    .text_color(rgb(0xffffff))
                                    .child("OK")
                                    .on_mouse_down(MouseButton::Left, |_, window, _| window.remove_window()),
                            ),
                    ),
            )
    }
}
