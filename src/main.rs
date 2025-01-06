use app_global::AppGlobal;
use gpui::*;
use std::{path::PathBuf, process::exit};

pub mod line_edit;
pub mod dialog;
pub mod models;
pub mod views;
pub mod app_global;

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

        AppGlobal::new_main_window(target, cx);
    });
}
