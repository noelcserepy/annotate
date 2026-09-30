use gpui::{Context, ObjectFit, Window, div, img, prelude::*, px, rgb, rgba};

use crate::{editor, history};

pub struct HistoryView {
    entries: Vec<history::Entry>,
}

impl HistoryView {
    pub fn new() -> Self {
        Self { entries: history::list() }
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.entries = history::list();
        cx.notify();
    }
}

impl Render for HistoryView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tiles = self.entries.iter().map(|entry| {
            let open_id = entry.id.clone();
            let delete_id = entry.id.clone();
            div()
                .id(gpui::SharedString::from(entry.id.clone()))
                .group("tile")
                .relative()
                .w(px(200.))
                .h(px(140.))
                .p_1()
                .rounded_lg()
                .bg(rgb(0x2c2c2e))
                .hover(|s| s.bg(rgb(0x3a3a3c)))
                .cursor_pointer()
                .child(img(entry.thumb.clone()).size_full().object_fit(ObjectFit::Contain))
                .on_click(move |_, _, cx| {
                    if let Some(capture) = history::load(&open_id) {
                        editor::open(capture, cx);
                    }
                })
                .child(
                    div()
                        .id("delete")
                        .absolute()
                        .top(px(6.))
                        .right(px(6.))
                        .size(px(20.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgba(0x000000b0))
                        .text_color(rgb(0xffffff))
                        .text_size(px(13.))
                        .opacity(0.)
                        .group_hover("tile", |s| s.opacity(1.))
                        .child("×")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            history::delete(&delete_id);
                            this.reload(cx);
                        })),
                )
        });

        div()
            .id("history")
            .size_full()
            .overflow_y_scroll()
            .bg(rgb(0x1c1c1e))
            .p_3()
            .child(div().flex().flex_wrap().gap_3().children(tiles))
    }
}
