use app_global::AppGlobal;
use gpui::*;
use std::path::PathBuf;

pub mod line_edit;
pub mod dialog;
pub mod models;
pub mod views;
pub mod app_global;

use models::DirModel;
use views::FileListView;

fn main() {
    App::new().run(|cx: &mut AppContext| {
        println!("Scanning icons and mime databases");
        cx.set_global(AppGlobal::new());
        println!("Done");

        let bounds = Bounds::centered(None, size(px(426.), px(480.)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |cx| {
                let model = cx.new_model(|_| DirModel::new(PathBuf::from("/"), false));
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
