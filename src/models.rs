use smol::channel::{Receiver, RecvError, Sender};
use smol::prelude::*;
use gpui::{BackgroundExecutor, ModelContext, SharedString, Task};
use smol::process::Command;
use std::cmp;
use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs::{DirEntry};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use crate::app_global::AppGlobal;

pub struct DirHistoryItem {
    current: Option<OsString>,
    path: PathBuf,
}

pub struct DirModel {
    pub dir_path: PathBuf,
    pub entries: Vec<DirEntry>,
    pub current: Option<usize>,
    pub marked: BTreeSet<usize>,
    pub history: Vec<DirHistoryItem>,
    pub start_with: String,
    pub show_hidden: bool,
}

pub struct DialogAction {
    pub text: String,
    pub key: String,
}

impl DialogAction {
    pub fn new(text: &str, key: &str) -> Self {
        Self {
            text: text.to_string(),
            key: key.to_string(),
        }
    }

    pub fn yes_no() -> Vec<Self> {
        vec![Self::new("Yes", "y"), Self::new("No", "n")]
    }

    pub fn multi_yes_no() -> Vec<Self> {
        vec![Self::new("All", "!"), Self::new("Yes", "y"),
             Self::new("No", "n"), Self::new("Cancel", "ctrl-g")]
    }
}


pub struct DialogOption {
    pub text: SharedString,
    pub icon_name: String,
}

pub struct DialogRequest {
    pub msg: SharedString,
    pub actions: Vec<DialogAction>,
    pub sel_option: Option<usize>,
    pub options: Vec<DialogOption>,
}

impl DialogRequest {
    pub fn new(msg: SharedString, actions: Vec<DialogAction>) -> Self {
        Self {
            msg, actions,
            sel_option: None,
            options: vec![],
        }
    }
}

#[derive(Clone, PartialEq, serde_derive::Deserialize)]
pub struct DialogResponse {
    pub action: usize,
    pub sel_option: Option<usize>
}

impl DialogResponse {
    pub fn new(action: usize, sel_option: Option<usize>) -> Self {
        Self {
            action,
            sel_option,
        }
    }
}

pub struct IOWorker<T: Send + 'static> {
    pub desc: String,
    pub result: Task<Result<T, String>>,
    pub ui: Receiver<DialogRequest>,
    pub input: Sender<DialogResponse>,
}

impl<T: Send + 'static> IOWorker<T> {
    pub fn spawn<Fut>(exe: &BackgroundExecutor, info: &str, fun: impl FnOnce(Sender<DialogRequest>, Receiver<DialogResponse>) -> Fut) -> Result<Self, String>
    where Fut: Future<Output = Result<T, String>> + Send + 'static {
        let ui_chan = smol::channel::unbounded();
        let input_chan = smol::channel::unbounded();
        Ok(Self {
            desc: info.to_string(),
            result: exe.spawn(fun(ui_chan.0, input_chan.1)),
            ui: ui_chan.1,
            input: input_chan.0,
        })
    }
    pub fn err(err: &str) -> Result<Self, String> {
        Err(err.to_string())
    }
}

pub async fn worker_dialog(request: DialogRequest,
                           ui_send: &Sender<DialogRequest>,
                           input_recv: &Receiver<DialogResponse>) -> Result<DialogResponse, RecvError> {
    while !input_recv.is_empty() {
        let _ = input_recv.recv().await;
    }
    ui_send.send(request).await.expect("Cannot send to main thread");
    input_recv.recv().await
}

pub async fn worker_error(err: SharedString,
                          ui_send: &Sender<DialogRequest>,
                          input_recv: &Receiver<DialogResponse>) {
    worker_dialog(
        DialogRequest::new(err, vec![DialogAction::new("OK", "enter")]),
        ui_send,
        input_recv).await.expect("Cannot receive from main thread");
}

pub async fn worker_multi_yes_no(msg: SharedString, existing_response: &mut Option<bool>,
                                 ui_send: &Sender<DialogRequest>, input_recv: &Receiver<DialogResponse>) -> bool {
    if existing_response.is_none() {
        let response = worker_dialog(
            DialogRequest::new(msg, DialogAction::multi_yes_no()),
            ui_send,
            input_recv).await.unwrap();

        if response.action == 0 {
            *existing_response = Some(true);
        } else if response.action == 3 {
            *existing_response = Some(false);
        }

        response.action < 2
    } else {
        existing_response.unwrap()
    }
}

pub async fn worker_progress(info: SharedString, last_progress_ts: &mut SystemTime, ui_send: &Sender<DialogRequest>) {
    let now = SystemTime::now();
    let Ok(duration) = now.duration_since(last_progress_ts.clone()) else {
        return;
    };
    if duration < Duration::from_millis(10) {
        return;
    }

    let _ = ui_send.send(DialogRequest::new(info, vec![DialogAction::new("Cancel", "ctrl-g")])).await;
    *last_progress_ts = now;
}

pub async fn worker_should_exit(input_recv: &Receiver<DialogResponse>) -> bool {
    if !input_recv.is_empty() {
        input_recv.recv().await.expect("Cannot receive from main thread");
        true
    } else {
        false
    }
}

pub struct OpenDirResult {
    path: PathBuf,
    entries: Vec<DirEntry>,
    current: Option<OsString>,
}

impl DirModel {
    fn load_entry_as_paths(p: &Path) -> std::io::Result<Vec<PathBuf>> {
        std::fs::read_dir(p).and_then(|entries| {
            let mut has_err: Option<std::io::Error> = None;
            let entries: Vec<_> = entries.filter_map(|e| {
                if e.is_err() {
                    has_err = e.err();
                    None
                } else {
                    Some(e.unwrap().path())
                }
            }).collect();
            if has_err.is_some() {
                Err(has_err.unwrap())
            } else {
                Ok(entries)
            }
        })
    }

    fn load_entries(path: &Path, show_hidden: bool) -> Vec<DirEntry> {
        let mut entries = std::fs::read_dir(path)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| show_hidden || entry.file_name().as_encoded_bytes()[0] != b'.')
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

    pub fn new(dir_path: PathBuf, show_hidden: bool) -> Self {
        Self {
            entries: Self::load_entries(&dir_path, show_hidden),
            current: None,
            marked: BTreeSet::new(),
            dir_path,
            history: vec![],
            start_with: String::new(),
            show_hidden,
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

    pub fn toggle_hidden(&mut self, _cx: &mut ModelContext<Self>) {
        self.show_hidden = !self.show_hidden;
        self.marked = BTreeSet::new();
        let cur_filename = self.current.map(|idx| self.entries[idx].file_name());
        self.entries = Self::load_entries(&self.dir_path, self.show_hidden);
        if let Some(last_filename) = cur_filename {
            self.current = self.entries.iter().position(|ent| ent.file_name() == last_filename);
        }
    }

    pub fn should_open_dir(&self) -> Option<bool> {
        self.current.and_then(|idx| self.entries[idx].file_type().ok().map(|t| t.is_dir()))
    }

    pub fn open_file(&mut self, cx: &mut ModelContext<Self>) -> Result<IOWorker<Option<(String, usize)>>, String> {
        let cur_idx = self.current.expect("BUG: use should_open_dir()");
        let mime = cx.global::<AppGlobal>().match_mime_type(
            self.entries[cur_idx].file_name().to_str().unwrap());

        let Some(assoc) = cx.global::<AppGlobal>().get_mime_assoc_index(&mime) else {
            return IOWorker::err("Cannot find an application to open this file.");
        };
        let mut all = assoc.all.clone();
        let sel_idx = if let Some(default_idx) = assoc.default {
            all.iter().position(|x| *x == default_idx).or_else(|| {
                all.insert(0, default_idx);
                Some(0)
            })
        } else {
            None
        };
        let options = all.iter().map(|idx| {
            DialogOption {
                text: cx.global::<AppGlobal>().get_menu_item(*idx).name.clone().into(),
                icon_name: cx.global::<AppGlobal>().get_menu_item(*idx).icon.clone(),
            }
        }).collect::<Vec<_>>();

        let cmds = all.iter().map(|idx| {
            let path = self.entries[cur_idx].path();
            let v = vec![&path];
            cx.global::<AppGlobal>().get_menu_item(*idx).detail_entry().unwrap().exec_with_filenames(&v)
        }).flatten().collect::<Vec<_>>();

        return IOWorker::spawn(
            cx.background_executor(),
            "Open file",
            |ui_send, input_recv| async move {
                let response = worker_dialog(DialogRequest {
                    msg: "Open file with:".into(),
                    actions: vec![
                        DialogAction::new("Default", "!"),
                        DialogAction::new("Yes", "enter"),
                        DialogAction::new("Cancel", "ctrl-g"),
                    ],
                    sel_option: sel_idx,
                    options
                }, &ui_send, &input_recv).await.unwrap();
                // Cancel
                if response.action == 2 {
                    return Ok(None);
                }

                let Some(sel_option) = response.sel_option else {
                    return Err("Did not selection an application".to_string());
                };

                if let Err(err) = Command::new("/bin/sh").arg("-c").arg(&cmds[sel_option]).spawn() {
                    return Err(err.to_string());
                }
                if response.action == 0 {
                    // Set the default
                    return Ok(Some((mime, all[sel_option])));
                } else {
                    return Ok(None);
                }
            });
    }

    pub fn after_open_file_result(default: Option<(String, usize)>, cx:&mut ModelContext<Self>) {
        if let Some((mime, idx)) = default {
            cx.global_mut::<AppGlobal>().write_default_assoc(&mime, idx);
        }
    }

    pub fn open_dir(&mut self, cx: &mut ModelContext<Self>) -> Result<IOWorker<OpenDirResult>, String> {
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

        let cur_ent = &self.entries[self.current.expect("BUG: use should_open_dir")];
        let Ok(file_type) = cur_ent.file_type() else {
            return IOWorker::err("Cannot determine file type");
        };

        let target_path = cur_ent.path().clone();
        let show_hidden = self.show_hidden;
        IOWorker::spawn(
            cx.background_executor(),
            "Reading directory...",
            |ui_send, _input_recv| async move {
                ui_send.close();
                if file_type.is_dir() {
                    let entries = Self::load_entries(&target_path, show_hidden);
                    return Ok(OpenDirResult {
                        path:target_path,
                        entries,
                        current: None
                    });
                } else if file_type.is_symlink() {
                    if let Ok(path) = resolve_symlink(target_path) {
                        if !path.exists() {
                            return Err(format!("Cannot follow {}", path.display()));
                        }
                        if path.is_dir() {
                            let entries = Self::load_entries(&path, show_hidden);
                            return Ok(OpenDirResult {
                                path,
                                entries,
                                current: None,
                            });
                        } else if path.is_file() {
                            let focus = path.file_name().map(|x| x.to_owned());
                            let path = path.parent().unwrap().to_path_buf();
                            let entries = Self::load_entries(&path, show_hidden);
                            return Ok(OpenDirResult {
                                path,
                                entries,
                                current: focus,
                            })
                        }
                    }
                }
                return Err("Do not know how to handle this item".to_string());
            })
    }

    pub fn refresh_with_result(&mut self, result: OpenDirResult) {
        self.dir_path = result.path;
        self.marked = BTreeSet::new();
        self.entries = result.entries;
        if let Some(name) = result.current {
            self.focus_file_name(&name);
        }
    }

    pub fn open_with_result(&mut self, result: OpenDirResult) {
        let path = std::mem::take(&mut self.dir_path);
        let current = std::mem::take(&mut self.current).map(|idx| self.entries[idx].file_name());
        self.history.push(DirHistoryItem { current, path });
        self.refresh_with_result(result);
    }

    pub fn back_with_result(&mut self, result: OpenDirResult) {
        self.history.pop();
        self.refresh_with_result(result);
    }

    pub fn focus_file_name(&mut self, name: &OsStr) {
        for i in 0..self.entries.len() {
            if self.entries[i].file_name() == name {
                self.current = Some(i);
                break;
            }
        }
    }

    pub fn back(&mut self, cx: &mut ModelContext<Self>) -> Result<IOWorker<OpenDirResult>, String> {
        let Some(ent) = self.history.last() else {
            return IOWorker::err("History empty");
        };
        let path = ent.path.clone();
        let current = ent.current.clone();
        let show_hidden = self.show_hidden;

        return IOWorker::spawn(
            cx.background_executor(),
            "Going back. Reading directory...",
            |ui_send, _input_recv| async move {
                // No need to report progress.
                ui_send.close();
                let entries = Self::load_entries(&path, show_hidden);
                Ok(OpenDirResult {
                    path,
                    entries,
                    current,
                })
            });
    }

    pub fn up(&mut self, cx: &mut ModelContext<Self>) -> Result<IOWorker<OpenDirResult>, String> {
        let mut path = self.dir_path.clone();
        if !path.pop() {
            return IOWorker::err(format!("Cannot go to the parent dir. {}", path.display()).as_str());
        }
        let show_hidden = self.show_hidden;
        return IOWorker::spawn(
            cx.background_executor(),
            "Moving up. Reading directory...",
            |ui_send, _input_recv| async move {
                ui_send.close();
                let entries = Self::load_entries(&path, show_hidden);
                Ok(OpenDirResult {
                    path,
                    entries,
                    current: None,
                })
            });
    }

    fn operate_items(&self) -> Vec<usize> {
        if self.marked.is_empty() {
            self.current.iter().cloned().collect()
        } else {
            self.marked.iter().cloned().collect()
        }
    }

    async fn delete_dir_entries(ui_send: &Sender<DialogRequest>, input_recv: &Receiver<DialogResponse>,
                                prefix_dir: &str, to_delete: Vec<PathBuf>,
                                file_response: &mut Option<bool>, dir_response: &mut Option<bool>,
                                last_progress_ts: &mut SystemTime,
                                exception_set: &BTreeSet<PathBuf>) -> bool {
        let nr_to_delete = to_delete.len();
        let mut nr_deleted = 0;

        for p in to_delete {
            if worker_should_exit(input_recv).await {
                break;
            }

            if exception_set.contains(&p) {
                continue;
            }

            let ent_name_osstring = p.file_name().unwrap();
            let ent_name = prefix_dir.to_string() + ent_name_osstring.to_str().unwrap_or("");
            let Ok(metadata) = p.symlink_metadata() else {
                worker_error(format!("Cannot read metadata of {}", ent_name).into(), ui_send, input_recv).await;
                continue;
            };
            if metadata.file_type().is_dir() {
                let should_delete = worker_multi_yes_no(
                    format!("Recursive delete directory {}?", ent_name).into(),
                    dir_response, ui_send, input_recv).await;

                if !should_delete {
                    continue;
                }

                let Ok(next_to_delete) = Self::load_entry_as_paths(&p) else {
                    worker_error(format!("Cannot read dir {}.", ent_name).into(), ui_send, input_recv).await;
                    continue;
                };

                let next_prefix_dir = ent_name.clone() + "/";

                let all_empty = Box::pin(Self::delete_dir_entries(
                    ui_send, input_recv, &next_prefix_dir, next_to_delete,
                    file_response, dir_response, last_progress_ts, exception_set)).await;

                if !all_empty {
                    continue;
                }

                if let Err(err) = std::fs::remove_dir(&p) {
                    worker_error(format!("Cannot remove dir {}. {}", ent_name, err).into(), ui_send, input_recv).await;
                    continue;
                }
            } else {
                let should_delete = worker_multi_yes_no(
                    format!("Delete {}?", ent_name).into(),
                    file_response, ui_send, input_recv).await;

                if !should_delete {
                    continue;
                }

                worker_progress(format!("Deleting {}", ent_name).into(), last_progress_ts, ui_send).await;

                if let Err(err) = std::fs::remove_file(p.as_path()) {
                    worker_error(format!("Cannot remove file {}, {}", ent_name, err).into(), ui_send, input_recv).await;
                    continue;
                }
            }
            nr_deleted += 1;
        }

        nr_deleted == nr_to_delete
    }


    pub fn delete(&mut self, cx: &mut ModelContext<Self>) -> Result<IOWorker<OpenDirResult>, String> {
        let to_delete = self.operate_items();
        if to_delete.is_empty() {
            return IOWorker::err("Nothing to delete");
        }

        let to_delete: Vec<_> = to_delete.iter().map(|idx| self.entries[*idx].path()).collect();
        let path = self.dir_path.clone();
        let current = self.current.map(|cur| self.entries[cur].file_name().clone());
        let show_hidden = self.show_hidden;

        return IOWorker::spawn(
            cx.background_executor(),
            "Deleting...",
            |ui_send, input_recv| async move {
                let mut file_response: Option<bool> = None;
                let mut dir_response: Option<bool> = None;
                let mut last_progress_ts = SystemTime::now() - Duration::from_millis(10);
                let exception_set = BTreeSet::new();

                Self::delete_dir_entries(
                    &ui_send, &input_recv,
                    "", to_delete,
                    &mut file_response, &mut dir_response,
                    &mut last_progress_ts,
                    &exception_set).await;

                let entries = Self::load_entries(&path, show_hidden);
                Ok(OpenDirResult {
                    path,
                    entries,
                    current,
                })
            });
    }

    async fn paste_entries(ui_send: &Sender<DialogRequest>, input_recv: &Receiver<DialogResponse>,
                           path: &Path, prefix_dir: &str, to_paste: Vec<PathBuf>, should_move: bool,
                           fail_set: &mut BTreeSet<PathBuf>, file_response: &mut Option<bool>,
                           last_progress_ts: &mut SystemTime) {
        let mut try_link = should_move;
        for p in to_paste {
            if worker_should_exit(input_recv).await {
                break;
            }

            let ent_name_osstring = p.file_name().unwrap();
            let ent_name = prefix_dir.to_string() + ent_name_osstring.to_str().unwrap_or("");
            let Ok(metadata) = p.symlink_metadata() else {
                fail_set.insert(p);
                worker_error(format!("Cannot read metadata of {}", ent_name).into(), ui_send, input_recv).await;
                continue;
            };

            let mut target = path.to_path_buf();
            target.push(ent_name_osstring);

            println!("target {}", target.display());

            if target.exists() {
                let Ok(target_metadata) = target.symlink_metadata() else {
                    fail_set.insert(p);
                    worker_error(format!("{} exists but cannot read its metadata", ent_name).into(), ui_send, input_recv).await;
                    continue;
                };
                if target_metadata.file_type() != metadata.file_type() {
                    fail_set.insert(p);
                    worker_error(format!("{} has different type than its original", ent_name).into(), ui_send, input_recv).await;
                    continue;
                }
                if !target_metadata.is_dir() {
                    let should_overwrite = worker_multi_yes_no(
                        format!("Overwrite existing file {}?", ent_name).into(),
                        file_response, ui_send, input_recv).await;
                    if !should_overwrite {
                        println!("not overwritting {}", ent_name);
                        fail_set.insert(p);
                        continue;
                    }
                }
            }

            worker_progress(format!("{} {}", if should_move { "Moving" } else { "Copying" },ent_name).into(),
                            last_progress_ts, ui_send).await;

            if metadata.is_dir() {
                if !target.exists() {
                    if let Err(err) = std::fs::create_dir(&target) {
                        fail_set.insert(p);
                        worker_error(format!("Cannot create {}, {}", ent_name, err).into(), ui_send, input_recv).await;
                        continue;
                    }
                }
                let Ok(entries) = Self::load_entry_as_paths(&p) else {
                    fail_set.insert(p);
                    worker_error(format!("Cannot read original dir {}", ent_name).into(), ui_send, input_recv).await;
                    continue;
                };
                let next_prefix_dir = ent_name.clone() + "/";
                Box::pin(Self::paste_entries(ui_send, input_recv, &target, &next_prefix_dir, entries, should_move, fail_set, file_response, last_progress_ts)).await;
            } else {
                if try_link {
                    if std::fs::hard_link(&p, &target).is_err() {
                        try_link = false;
                    } else {
                        continue;
                    }
                }
                if let Err(err) = std::fs::copy(&p, &target) {
                    fail_set.insert(p);
                    worker_error(format!("Cannot copy {}, {}", ent_name, err).into(), ui_send, input_recv).await;
                    continue;
                }
            }
        }
    }

    pub fn paste(&mut self, cx: &mut ModelContext<Self>) -> Result<IOWorker<OpenDirResult>, String> {
        let path = self.dir_path.clone();
        let current = self.current.map(|cur| self.entries[cur].file_name().clone());
        let show_hidden = self.show_hidden;
        let to_paste = cx.global_mut::<AppGlobal>().take_stash();
        let should_move = cx.global::<AppGlobal>().is_stash_move();
        return IOWorker::spawn(
            cx.background_executor(),
            "Pasting...",
            |ui_send, input_recv| async move {
                let mut file_response: Option<bool> = None;
                let mut fail_set = BTreeSet::new();
                let mut last_progress_ts = SystemTime::now() - Duration::from_millis(10);

                Self::paste_entries(&ui_send, &input_recv,
                                    &path, "", to_paste.clone(), should_move,
                                    &mut fail_set,
                                    &mut file_response,
                                    &mut last_progress_ts).await;

                if should_move && !worker_should_exit(&input_recv).await {
                    for ent in &fail_set {
                        println!("fail set {}", ent.display());
                    }

                    let mut dir_response = Some(true); // Always delete without asking.
                    file_response = Some(true);

                    Self::delete_dir_entries(&ui_send, &input_recv,
                                             "", to_paste,
                                             &mut file_response,
                                             &mut dir_response,
                                             &mut last_progress_ts,
                                             &fail_set).await;
                }

                let entries = Self::load_entries(&path, show_hidden);
                Ok(OpenDirResult {
                    path,
                    entries,
                    current,
                })
            });
    }

    pub fn copy_or_move(&mut self, cx: &mut ModelContext<Self>, should_move: bool) {
        let stash: Vec<_> = self.operate_items().iter().map(|idx| {
            self.entries[*idx].path()
        }).collect();
        cx.global_mut::<AppGlobal>().stash(stash, should_move);
    }

    pub fn rename(&mut self, cx: &mut ModelContext<Self>, new_name: String) -> Result<IOWorker<OpenDirResult>, String> {
        let Some(cur) = self.current else {
            return IOWorker::err("Nothing selected.");
        };
        let src = self.entries[cur].path();
        let show_hidden = self.show_hidden;
        let path = self.dir_path.clone();

        IOWorker::spawn(
            cx.background_executor(),
            "Renaming",
            |ui_send, input_recv| async move {
                let mut target = src.clone();
                target.pop();
                target.push(&new_name);

                // We need to perform this in IOWorker because it may block on NFS.
                if let Err(err) = std::fs::rename(&src, &target) {
                    worker_error(
                        format!("Cannot rename {}, {}", src.file_name().unwrap().to_string_lossy(), err).into(),
                        &ui_send,
                        &input_recv).await;
                    return Err("Rename failed".to_string());
                }
                let entries = Self::load_entries(&path, show_hidden);
                Ok(OpenDirResult {
                    path,
                    entries,
                    current: Some(OsString::from_str(&new_name).unwrap()),
                })
            })
    }
}
