use gpui::*;
use std::path::PathBuf;

pub mod line_edit;
pub mod models;
pub mod views;
pub mod icon_source;

use models::CurrentDirModel;
use views::FileListView;

fn main() {
    App::new().run(|cx: &mut AppContext| {
        let bounds = Bounds::centered(None, size(px(426.), px(480.)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |cx| {
                let model = cx.new_model(|_| CurrentDirModel::new(PathBuf::from("/")));
                let view = cx.new_view(|cx| {
                    let mut view = FileListView::new(cx, model);
                    view.on_navigate(cx);
                    view
                });
                cx.focus_view(&view);
                view
            },
        )
        .unwrap();
    });
}
