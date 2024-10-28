use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::{collections::HashMap, ffi::OsString, fs::File};
use regex::Regex;
use memmap::MmapOptions;
use xdg_desktop::icon::{IconDescription, IconIndex};
use gpui::ImageSource;

struct MIMEGlobItem {
    mime: OsString,
    reg: Regex,
}

pub struct IconSource {
    glob_regs: Vec<MIMEGlobItem>,
    glob_suffix_index: HashMap<OsString, OsString>,
    icon_index: IconIndex,
}

fn parse_mime_glob<'a, Callback>(slice: &'a [u8], mut callback: Callback) where Callback: FnMut(&'a [u8], &'a [u8]) -> () {
    let mut line_start = 0;
    loop {
        if line_start == slice.len() {
            break;
        }
        if slice[line_start] == b'#' {
            if let Some(line_end) = slice[line_start..].iter().position(|ch| *ch == b'\n') {
                line_start += line_end + 1;
                continue;
            } else {
                break;
            }
        }
        if let Some(colon_pos) = slice[line_start..].iter().position(|ch| *ch == b':') {
            if line_start + colon_pos + 1 == slice.len() {
                break;
            }
            if let Some(line_end) = slice[line_start + colon_pos + 1..].iter().position(|ch| *ch == b'\n') {
                let mime = &slice[line_start..line_start + colon_pos];
                let reg = &slice[line_start + colon_pos + 1..line_start + colon_pos + 1 + line_end];
                callback(mime, reg);

                line_start += colon_pos + line_end + 2;
            } else {
                break;
            }
        } else {
            break;
        }
    }
}

impl IconSource {
    pub fn new() -> Self {
        let mut glob_regs = Vec::new();
        let mut glob_suffix_index = HashMap::new();

        if let Ok(region) = File::open("/usr/share/mime/globs").and_then(|file| {
            unsafe { MmapOptions::new().map(&file) }
        }) {
            parse_mime_glob(region.iter().as_slice(), |mime, reg| {
                println!("mime {} pattern {}", OsStr::from_bytes(mime).to_str().unwrap(), OsStr::from_bytes(reg).to_str().unwrap());
                if reg[0] == b'*' {
                    glob_suffix_index.insert(
                        OsStr::from_bytes(&reg[1..]).to_owned(),
                        OsStr::from_bytes(mime).to_owned());
                } else {
                    let reg_str = String::from_utf8_lossy(reg).into_owned();
                    glob_regs.push(MIMEGlobItem {
                        mime: OsStr::from_bytes(mime).to_owned(),
                        reg: Regex::new(&reg_str).unwrap(),
                    });
                }
            });
        } else {
            eprintln!("Cannot find or open mime database.");
        }
        let mut icon_index = IconIndex::new();
        let dirs = std::env::var("XDG_DATA_DIRS").unwrap_or(String::from("/usr/share:/usr/local/share"));
        icon_index.scan_with_theme(vec!["Papirus", "hicolor"], &dirs.split(":").collect());

        Self {
            glob_regs, glob_suffix_index,
            icon_index,
        }
    }

    fn match_filename_suffix(&self, filename: &str) -> Option<&OsStr> {
        if let Some(extpos) = filename.rfind('.') {
            let extosstr = OsStr::from_bytes(&filename[extpos..].as_bytes());
            if let Some(mime) = self.glob_suffix_index.get(extosstr) {
                return Some(mime);
            }
        }

        None
    }

    fn match_filename_regex(&self, filename: &str) -> Option<&OsStr> {
        for glob_item in &self.glob_regs {
            if glob_item.reg.is_match(filename) {
                return Some(&glob_item.mime);
            }
        }

        None
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

    pub fn match_filename(&self, filename: &str, size: usize, scale: f32) -> ImageSource {
        let mime = self.match_filename_suffix(filename)
            .unwrap_or_else(|| self.match_filename_regex(filename).unwrap_or(OsStr::from_bytes(b"application/x-generic")))
            .to_str()
            .unwrap()
            .replace('/', "-");

        self.match_icon(&mime, size, scale).unwrap_or_else(|| {
            self.match_icon("application-x-generic", size, scale).unwrap_or_else(
                || -> ImageSource {
                    eprintln!("Cannot even find icon for {}?", &mime);
                    PathBuf::from("").into()
                })
        })
    }


    pub fn match_directory(&self, size: usize, scale: f32) -> ImageSource {
        let mime = "folder";
        self.match_icon(&mime, size, scale).unwrap_or_else(|| -> ImageSource {
            eprintln!("Cannot even find icon for folder?");
            PathBuf::from("").into()
        })
    }
}
