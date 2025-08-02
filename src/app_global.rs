use std::fs::File;
use std::io::{Error, Read};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use futures::Future;
use image::{Frame, ImageBuffer};
use smallvec::SmallVec;
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

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
struct CustomSizeSvg {
    path: PathBuf,
    actual_size: i32,
}

struct CustomSizeAvgAsset {}

impl Asset for CustomSizeAvgAsset {
    type Source = CustomSizeSvg;
    type Output = Result<Arc<RenderImage>, ImageCacheError>;

    fn load(source: Self::Source, _cx: &mut App) -> impl Future<Output = Self::Output> + Send + 'static {
        let mut buf = Vec::new();
        let p = source.path.clone();
        async move {
            let Ok(mut f) = File::open(p) else {
                return Err(ImageCacheError::Io(Arc::new(Error::last_os_error())));
            };
            f.read_to_end(&mut buf).unwrap();
            let tree = usvg::Tree::from_data(buf.as_slice(), &usvg::Options::default());
            if tree.is_err() {
                return Err(ImageCacheError::Usvg(Arc::new(tree.err().unwrap())));
            }
            let tree = tree.unwrap();
            let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(source.actual_size as u32, source.actual_size as u32) else {
                return Err(ImageCacheError::Io(Arc::new(Error::last_os_error())));
            };
            let scale = source.actual_size as f32 / tree.size().width();
            let transform = resvg::tiny_skia::Transform::from_scale(scale, scale);
            resvg::render(&tree, transform, &mut pixmap.as_mut());

            let mut buffer = ImageBuffer::from_raw(pixmap.width(), pixmap.height(), pixmap.take()).unwrap();
            for pixel in buffer.chunks_exact_mut(4) {
                pixel.swap(0, 2);
                if pixel[3] > 0 {
                    let a = pixel[3] as f32 / 255.;
                    pixel[0] = (pixel[0] as f32 / a) as u8;
                    pixel[1] = (pixel[1] as f32 / a) as u8;
                    pixel[2] = (pixel[2] as f32 / a) as u8;
                }
            }

            Ok(Arc::new(RenderImage::new(SmallVec::from_elem(Frame::new(buffer), 1))))
        }
    }
}

impl Global for AppGlobal {}

impl AppGlobal {
    pub fn new() -> Self {
        let mut icon_index = IconIndex::new();
        let home_dir = std::env::var("HOME").unwrap();

        let mut res_path = std::env::current_exe().unwrap();
        res_path.pop();
        res_path.pop();
        res_path.push("Resources/");

        let dirs = if cfg!(target_os = "linux") {
            std::env::var("XDG_DATA_DIRS").unwrap_or(String::from(home_dir.clone() + "/.local/share:/usr/share:/usr/local/share"))
        } else if cfg!(target_os = "macos") {
            String::from(home_dir.clone() + "/.local/share:" + res_path.as_os_str().to_str().unwrap())
        } else {
            panic!("Unsupported platform");
        };

        let mut unique_dir = HashSet::new();
        let paths = dirs.split(":").filter_map(|s| if unique_dir.contains(s) { None } else { unique_dir.insert(s); Some(Path::new(s)) });

        let config_path = home_dir + "/.config/forg.toml";
        let mut theme = if cfg!(target_os = "linux") {
            "Adwaita".to_string()
        } else if cfg!(target_os = "macos") {
            "Papirus".to_string()
        } else {
            panic!("Unsupported platform");
        };

        if let Ok(config_str) = std::fs::read_to_string(config_path) {
            let config = toml::from_str::<Table>(&config_str).expect("Cannot parse forg.toml!");
            config["icon-theme"].as_str().map(|name| { theme = name.to_string(); });
        }

        icon_index.scan_with_theme(vec![&theme, "hicolor"], paths);

        let mime_index = if cfg!(target_os = "linux") {
            MIMEGlobIndex::new().unwrap()
        } else if cfg!(target_os = "macos") {
            let mut globs_path = res_path.clone();
            globs_path.push("mime");
            globs_path.push("globs2");
            println!("{}", globs_path.display());
            MIMEGlobIndex::new_with_path(globs_path).unwrap()
        } else {
            panic!("");
        };

        let mut menu_index = MenuIndex::new_default();

        // Do not scan for DesktopEntries under Mac.
        if cfg!(target_os = "linux") {
            menu_index.scan();
        }

        let cur_stash = vec![];

        Self {
            mime_index,
            icon_index,
            menu_index,
            cur_stash,
            cur_stash_move: false,
        }
    }

    fn load_image(p: PathBuf, actual_size: i32) -> ImageSource {
        if p.extension().is_some_and(|ext| ext == "svg") {
            // We can't use the default image source loader because
            // GPUI will rasterize according to the SVG file size and
            // these file sizes may not be correct. After all SVG is
            // scalable.
            ImageSource::from(move |window: &mut Window, cx: &mut App| {
                let cus_svg = CustomSizeSvg { path: p.clone(), actual_size };
                window.use_asset::<CustomSizeAvgAsset>(&cus_svg, cx)
            })
        } else {
            p.into()
        }
    }

    pub fn match_icon(&self, mime: &str, size: usize, scale: f32) -> Option<ImageSource> {
        let actual_size = (size as f32 * scale).ceil() as i32;

        self.icon_index.index.get(mime).map(move |icons| -> ImageSource {
            let mut mindiff = i32::MAX;
            let mut candidate = PathBuf::new();
            for icon in icons {
                if let IconDescription::Bitmap(bitmap_desc) = &icon.desc {
                    let diff = actual_size - (bitmap_desc.size * bitmap_desc.scale) as i32;
                    if diff > 0 {
                        if diff < mindiff {
                            mindiff = diff;
                            candidate = icon.path.clone();
                        }
                        continue;
                    }
                }
                return Self::load_image(icon.path.clone(), actual_size);
            }
            return Self::load_image(candidate.clone(), actual_size);
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

    pub fn new_main_window(target: PathBuf, cx: &mut AsyncApp) {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(460.), px(480.)));

        let _handle = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some("forg".to_string()),
                focus: true,
                show: true,
                ..Default::default()
            },
            |window, cx| {
                let model = cx.new(|_| DirModel::new(target, false));
                let view = cx.new(|cx| {
                    let mut view = FileListView::new(window, cx, model);
                    view.on_navigate(window, cx);
                    view
                });
                view.focus_handle(cx).focus(window);

                view
            },
        ).unwrap();
    }
}
