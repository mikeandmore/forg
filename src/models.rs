use gpui::ModelContext;
use std::cmp;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::DirEntry;
use std::ops::Range;
use std::path::{Path, PathBuf};

pub struct CurrentDirModel {
    pub dir_path: PathBuf,
    pub entries: Vec<DirEntry>,
    pub current: Option<usize>,
    pub marked: BTreeSet<usize>,
    pub history: Vec<PathBuf>,
    pub start_with: String,

}

impl CurrentDirModel {
    fn load_entries(path: &Path) -> Vec<DirEntry> {
        let mut entries = std::fs::read_dir(path)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .collect::<Vec<DirEntry>>();
        entries.sort_by(|p, q| {
            if let (Ok(pf), Ok(qf)) = (p.file_type(), q.file_type()) {
                if pf.is_dir() && !qf.is_dir() {
                    return cmp::Ordering::Less;
                } else if !pf.is_dir() && qf.is_dir() {
                    return cmp::Ordering::Greater;
                }
            }
            p.file_name().cmp(&q.file_name())
        });

        entries
    }

    pub fn new(dir_path: PathBuf) -> Self {
        Self {
            entries: Self::load_entries(&dir_path),
            current: None,
            marked: BTreeSet::new(),
            dir_path,
            history: vec![],
            start_with: String::new(),
        }
    }

    pub fn move_next(&mut self, _: &mut ModelContext<Self>) {
        if !self.entries.is_empty() {
            self.current = Some(
                self.current
                    .map_or(0, |v| std::cmp::min(v + 1, self.entries.len() - 1)),
            );
        }
    }

    pub fn search_next(&mut self, _: &mut ModelContext<Self>) -> bool {
        let do_search = |this: &mut Self, range: Range<usize>| -> bool {
            for idx in range {
                if let Some(fname) = this.entries[idx].file_name().to_str() {
                    if fname.starts_with(&this.start_with) {
                        this.current = Some(idx);
                        return true;
                    }
                }
            }
            return false;
        };

        if let Some(cur) = self.current {
            do_search(self, (cur + 1)..self.entries.len()) || do_search(self, 0..(cur + 1))
        } else {
            do_search(self, 0..self.entries.len())
        }
    }

    pub fn search_clear(&mut self, _: &mut ModelContext<Self>) {
        self.start_with.clear();
    }

    pub fn move_prev(&mut self, _: &mut ModelContext<Self>) {
        self.current = self
            .current
            .map_or(None, |v| Some(if v == 0 { 0 } else { v - 1 }));
    }

    pub fn move_home(&mut self, _: &mut ModelContext<Self>) {
        if !self.entries.is_empty() {
            self.current = Some(0);
        }
    }

    pub fn move_end(&mut self, _: &mut ModelContext<Self>) {
        if !self.entries.is_empty() {
            self.current = Some(self.entries.len() - 1);
        }
    }

    pub fn toggle_mark(&mut self, cx: &mut ModelContext<Self>) {
        if let Some(cur) = self.current {
            if self.marked.contains(&cur) {
                self.marked.remove(&cur);
            } else {
                self.marked.insert(cur);
            }
        }
        self.move_next(cx);
    }

    pub fn open(&mut self, _: &mut ModelContext<Self>) {
        let open_dir = |this: &mut Self, path: PathBuf| {
            this.history.push(std::mem::take(&mut this.dir_path));
            this.dir_path = path;
            this.current = None;
            this.marked = BTreeSet::new();
            this.entries = Self::load_entries(&this.dir_path);
        };

        let focus_ent = |this: &mut Self, name: &OsStr| {
            for i in 0..this.entries.len() {
                if this.entries[i].file_name() == name {
                    this.current = Some(i);
                    break;
                }
            }
        };

        let resolve_symlink = |path: PathBuf| -> Result<PathBuf, ()> {
            if let Ok(link_path) = path.read_link() {
                if link_path.is_relative() {
                    let mut real_path = path.parent().unwrap().to_owned();
                    real_path.push(link_path);

                    return Ok(real_path);
                } else {
                    return Ok(link_path.to_path_buf());
                }
            } else {
                return Err(());
            }
        };

        if let Some(cur) = self.current {
            let cur_ent = &self.entries[cur];
            if let Ok(file_type) = cur_ent.file_type() {
                if file_type.is_dir() {
                    open_dir(self, cur_ent.path());
                } else if file_type.is_symlink() {
                    if let Ok(path) = resolve_symlink(cur_ent.path()) {
                        if !path.exists() {
                            println!("Cannot follow {}", path.display());
                            todo!("Show some error in GUI");
                        }
                        if path.is_dir() {
                            open_dir(self, path);
                        } else if path.is_file() {
                            open_dir(self, path.parent().unwrap().to_path_buf());
                            focus_ent(self, path.file_name().unwrap());
                        }
                    }
                }
            }
        }
    }

    pub fn back(&mut self, _: &mut ModelContext<Self>) {
        if let Some(path) = self.history.pop() {
            self.dir_path = path;
            self.current = None;
            self.marked = BTreeSet::new();
            self.entries = Self::load_entries(&self.dir_path);
        }
    }
}
