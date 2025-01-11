use std::path::{Path, PathBuf};
use toml::Table;
use xdg_desktop::icon::{IconDescription, IconIndex};
use xdg_desktop::menu::{MenuAssociation, MenuIndex, MenuItem};
use xdg_desktop::mime_glob::MIMEGlobIndex;

use gpui::*;

use crate::models::DirModel;
use crate::views::FileListView;

pub struct AppGlobal {
    mime_index: MIMEGlobIndex,
    pub icon_index: IconIndex,
    pub menu_index: MenuIndex,

    pub cur_stash: Vec<PathBuf>,
    pub cur_stash_move: bool,
}

impl Global for AppGlobal {}

impl AppGlobal {
    pub fn new() -> Self {
        let mut icon_index = IconIndex::new();
        let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or(String::from("/usr/share:/usr/local/share"));
        let paths = dirs.split(":").map(|s| Path::new(s));
        let config_path = std::env::var("HOME").unwrap() + "/.config/forg.toml";
        let mut theme = "Adwaita".to_string();
        if let Ok(config_str) = std::fs::read_to_string(config_path) {
            let config = toml::from_str::<Table>(&config_str).expect("Cannot parse forg.toml!");
            config["icon-theme"].as_str().map(|name| { theme = name.to_string(); });
        }

        icon_index.scan_with_theme(vec![&theme, "hicolor"], paths);

        let mime_index = MIMEGlobIndex::new().unwrap();
        let mut menu_index = MenuIndex::new_default();
        menu_index.scan();

        let cur_stash = vec![];

        Self {
            mime_index,
            icon_index,
            menu_index,
            cur_stash,
            cur_stash_move: false,
        }
    }

    pub fn match_icon(&self, mime: &str, size: usize, scale: f32) -> Option<ImageSource> {
        let actual_size = (size as f32 * scale) as usize;
        self.icon_index.index.get(mime).map(move |icons| -> ImageSource {
            let mut mindiff = usize::MAX;
            let mut candidate = PathBuf::new();
            for icon in icons {
                if let IconDescription::Bitmap(bitmap_desc) = &icon.desc {
                    let diff = actual_size - bitmap_desc.size * bitmap_desc.scale;
                    if diff > 0 {
                        if diff < mindiff {
                            mindiff = diff;
                            candidate = icon.path.clone();
                        }
                        continue;
                    }
                }
                return icon.path.clone().into();
            }
            return candidate.into();
        })
    }

    pub fn match_mime_type(&self, filename: &str) -> String {
        self.mime_index.match_filename(filename).unwrap_or("application/x-generic").to_string()
    }

    pub fn match_file_icon(&self, mime: &str, size: usize, scale: f32) -> ImageSource {
        let icon_name = mime.replace('/', "-");
        self.match_icon(&icon_name, size, scale).unwrap_or_else(|| {
            self.match_icon("application-x-generic", size, scale).unwrap_or_else(
                || -> ImageSource {
                    eprintln!("Cannot find icon {}", &icon_name);
                    PathBuf::from("").into()
                })
        })
    }


    pub fn match_directory_icon(&self, size: usize, scale: f32) -> ImageSource {
        let mime = "folder";
        self.match_icon(&mime, size, scale).unwrap_or_else(|| -> ImageSource {
            eprintln!("Cannot even find icon for folder?");
            PathBuf::from("").into()
        })
    }

    pub fn get_mime_assoc_index(&self, mime: &str) -> Option<&MenuAssociation> {
        self.menu_index.mime_assoc_index.get(mime)
    }

    pub fn get_menu_item(&self, idx: usize) -> &MenuItem {
        &self.menu_index.items[idx]
    }

    pub fn write_default_assoc(&mut self, mime: &str, idx: usize) {
        self.menu_index.change_default_assoc(mime, idx);
        self.menu_index.write_default_assoc().unwrap();
    }

    pub fn stash(&mut self, stash: Vec<PathBuf>, should_move: bool) {
        self.cur_stash = stash;
        self.cur_stash_move = should_move;
    }

    pub fn is_stash_move(&self) -> bool {
        self.cur_stash_move
    }

    pub fn take_stash(&mut self) -> Vec<PathBuf> {
        std::mem::take(&mut self.cur_stash)
    }

    pub fn new_main_window(target: PathBuf, cx: &mut AsyncAppContext) {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(460.), px(480.)));

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
        ).unwrap();
    }
}
