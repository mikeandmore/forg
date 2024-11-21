use std::path::{Path, PathBuf};
use xdg_desktop::icon::{IconDescription, IconIndex};
use xdg_desktop::mime_glob::MIMEGlobIndex;

use gpui::{Global, ImageSource};

pub struct AppGlobal {
    mime_index: MIMEGlobIndex,
    icon_index: IconIndex,
}

impl Global for AppGlobal {}

impl AppGlobal {
    pub fn new() -> Self {
        let mut icon_index = IconIndex::new();
        let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or(String::from("/usr/share:/usr/local/share"));
        icon_index.scan_with_theme(vec!["Papirus", "hicolor"], dirs.split(":").map(|s| Path::new(s)));
        let mime_index = MIMEGlobIndex::new().unwrap();

        Self {
            mime_index,
            icon_index,
        }
    }

    fn match_icon(&self, mime: &str, size: usize, scale: f32) -> Option<ImageSource> {
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
}
