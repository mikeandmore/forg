use app_global::AppGlobal;
use gpui::*;
use std::{path::PathBuf, process::exit};

pub mod line_edit;
pub mod dialog;
pub mod models;
pub mod views;
pub mod app_global;

use models::DirModel;
use views::FileListView;

fn main() {
    let target = PathBuf::from(std::env::args().nth(1).unwrap_or(std::env::var("HOME").unwrap()));
    if !target.exists() || !target.metadata().is_ok_and(|m| m.is_dir()) {
        eprintln!("{} is not a dir", target.display());
        exit(-1);
    }
    App::new().run(|cx: &mut AppContext| {
        println!("Scanning icons and mime databases");
        cx.set_global(AppGlobal::new());
        println!("Done");

        let bounds = Bounds::centered(None, size(px(460.), px(480.)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |cx| {
                let model = cx.new_model(|_| DirModel::new(target, false));
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
