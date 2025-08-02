use gpui::*;
use std::fs::DirEntry;
use std::ops::Range;

use crate::app_global::AppGlobal;
use crate::line_edit::{CommitEvent};
use crate::models::{DialogRequest, DialogResponse, IOWorker, OpenDirResult};
use super::line_edit::LineEdit;
use super::models::DirModel;
use super::dialog::Dialog;

#[derive(IntoElement)]
struct DirEntryView {
    id: usize,
    listview: Entity<FileListView>,
    icon: ImageSource,
    // mime_type: String,
    model: Entity<DirModel>,
    text_offset: f32,
}

impl DirEntryView {
    fn new(
        id: usize,
        icon: ImageSource,
        listview: Entity<FileListView>,
        _mime_type: String,
        model: Entity<DirModel>,
        text_offset: f32,
    ) -> Self {
        Self {
            id,
            listview,
            icon,
            // mime_type,
            model,
            text_offset,
        }
    }
}

static FILENAME_FALLBACK: &str = "Unrecognizable Unicode";

impl RenderOnce for DirEntryView {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let model = self.model.read(cx);
        let text = model.entries[self.id].file_name().into_string().unwrap_or(FILENAME_FALLBACK.to_string());
        let listview = self.listview.read(cx);
        let text_radius = listview.text_radius();
        let icon_size = listview.icon_size.clone();
        let margin_size = listview.margin_size();
        let font_size = listview.font_size();

        let mut label_div = div()
            .flex_none()
            .px(px(self.text_offset))
            .text_size(px(font_size))
            .rounded(px(text_radius))
            .child(text.clone());

        if model.current == Some(self.id) {
            label_div = label_div.bg(rgb(0x0068d9)).absolute().top(px(icon_size)).text_color(rgb(0xf0f0f0));
        } else {
            label_div = label_div.whitespace_nowrap().overflow_x_hidden().h(px(font_size + 2. * text_radius));
        }

        if let Ok(file_type) = model.entries[self.id].file_type() {
            if file_type.is_symlink() {
                label_div
                    .style()
                    .text_style()
                    .get_or_insert_with(Default::default)
                    .color = Some(rgb(0x47c8d6).into());
            }
        }

        let mut item_div = div()
            .id(self.id)
            .flex()
            .flex_col()
            .w(px(listview.text_width()))
            .m(px(margin_size))
            .child(
                img(self.icon.clone())
                    .flex_none()
                    .ml(px((listview.text_width() - icon_size) / 2.))
                    .w(px(icon_size))
                    .h(px(icon_size)),
            )
            .child(label_div);

        if model.marked.contains(&self.id) {
            item_div.style().background = Some(Fill::from(rgb(0xfff7a0)));
        }

        item_div
    }
}

// Actions
#[derive(Clone, PartialEq, serde_derive::Deserialize, schemars::JsonSchema, Action)]
enum ZoomAction {
    In, Out, Reset,
}

#[derive(Clone, PartialEq, serde_derive::Deserialize, schemars::JsonSchema, Action)]
enum MoveAction {
    Next, Prev, Home, End,
}

#[derive(Clone, PartialEq, serde_derive::Deserialize, schemars::JsonSchema, Action)]
struct CopyOrCut {
    should_move: bool
}

actions!(
    actions,
    [
        ToggleMark, ToggleHidden, Open, Remove, Paste, Rename, Up, Back, Search, Escape,
        NewWindow, CloseWindow
    ]
);

#[derive(PartialEq)]
pub enum StatusPrompt {
    Search,
    Rename,
}

impl StatusPrompt {
    fn to_str(&self) -> &'static str {
        match self {
            Self::Search => "Search: ",
            Self::Rename => "Rename: ",
        }
    }
}

pub struct FileListView {
    model: Entity<DirModel>,
    scroll_handle: UniformListScrollHandle,
    icon_size: f32,

    text_offset_cache: Vec<Option<f32>>,
    text_offset_cache_scale: f32,

    dialog: Entity<Dialog>,

    pub line_edit: Entity<LineEdit>,
    status_text: SharedString,
    status_prompt: Option<StatusPrompt>,

    focus_handle: FocusHandle,
    scroll_range: Range<usize>,
}

impl FileListView {
    fn on_dismiss<V>(&mut self, _source: &Entity<V>, _: &DismissEvent, window: &mut Window, cx: &mut Context<Self>) {
        println!("dismiss event reset");
        self.focus_handle.focus(window);
        self.line_edit.update(cx, |view, _| {
            view.reset();
        });
        self.update_view(window, cx, |view, _window, cx| {
            view.model.update(cx, &DirModel::search_clear);
            view.reset_status(cx);
        });
        Self::enter_mode(cx);
    }
    fn enter_mode(cx: &mut App) {
        cx.clear_key_bindings();
        cx.bind_keys([
            KeyBinding::new("n", MoveAction::Next, None),
            KeyBinding::new("p", MoveAction::Prev, None),
            KeyBinding::new(if cfg!(target_os = "macos") { "cmd-<" } else { "alt-<" }, MoveAction::Home, None),
            KeyBinding::new(if cfg!(target_os = "macos") { "cmd->" } else { "alt->" }, MoveAction::End, None),
            KeyBinding::new("m", ToggleMark, None),
            KeyBinding::new("h", ToggleHidden, None),
            KeyBinding::new("d", Remove, None),
            KeyBinding::new("r", Rename, None),
            KeyBinding::new("enter", Open, None),
            KeyBinding::new("backspace", Back, None),
            KeyBinding::new("^", Up, None),
            KeyBinding::new("ctrl-s", Search, None),
            KeyBinding::new("escape", Escape, None),
            KeyBinding::new("ctrl-g", Escape, None),
            KeyBinding::new("ctrl-w", CopyOrCut { should_move: true }, None),
            KeyBinding::new("alt-w", CopyOrCut { should_move: false }, None),
            KeyBinding::new("ctrl-y", Paste, None),
            KeyBinding::new("shift-n", NewWindow, None),
            KeyBinding::new("ctrl-x k", CloseWindow, None),

            KeyBinding::new("ctrl-=", ZoomAction::In, None),
            KeyBinding::new("ctrl--", ZoomAction::Out, None),
            KeyBinding::new("ctrl-0", ZoomAction::Reset, None),
        ]);
    }

    fn on_line_edit_commit(&mut self, edit: &Entity<LineEdit>, _: &CommitEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window);
        Self::enter_mode(cx);

        let Some(prompt) = &self.status_prompt else {
            return;
        };

        if *prompt == StatusPrompt::Search {
            self.update_view(window, cx, |view, _window, cx| {
                let result = view.model.update(cx, |model, cx| {
                    model.start_with = edit.read(cx).content.to_string();
                    model.search_next(cx)
                });
                if result {
                    view.status_text = SharedString::from(format!(
                        "Found at Location {}",
                        view.model.read(cx).current.unwrap()
                    ));
                } else {
                    view.status_text = SharedString::from("Not Found");
                }

            });
        } else if *prompt == StatusPrompt::Rename {
            let new_name = edit.read(cx).content.to_string();
            self.reset_status(cx);
            let worker = self.model.update(cx, |model, cx| model.rename(cx, new_name));
            self.update_with_io_worker(window, cx, worker, &Self::io_worker_refresh_callback);
        }
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>, model: Entity<DirModel>) -> Self {
        let focus_handle = cx.focus_handle();

        Self::enter_mode(cx);

        let line_edit = cx.new(|cx| LineEdit::new(window, cx));
        let dialog = cx.new(|cx| Dialog::new(window, cx));

        let scroll_handle = UniformListScrollHandle::new();

        cx.subscribe_in(&line_edit, window, Self::on_dismiss).detach();
        cx.subscribe_in(&dialog, window, Self::on_dismiss).detach();

        cx.subscribe_in(&line_edit, window, Self::on_line_edit_commit).detach();

        Self {
            model,
            scroll_handle,
            scroll_range: 0..0,
            icon_size: 64.,
            text_offset_cache: Vec::new(),
            text_offset_cache_scale: 0.,
            dialog,
            line_edit,
            status_text: "".into(),
            status_prompt: None,
            focus_handle,
        }
    }

    fn text_width(&self) -> f32 {
        self.icon_size * 1.5
    }
    fn margin_size(&self) -> f32 {
        self.icon_size / 8.
    }
    fn text_radius(&self) -> f32 {
        self.icon_size / 16.
    }
    fn font_size(&self) -> f32 {
        self.icon_size / 32. * 6.
    }

    pub fn zoom_in(&mut self) {
        if self.icon_size < 128. {
            self.icon_size += 16.;
        }
    }

    pub fn zoom_out(&mut self) {
        if self.icon_size > 16. {
            self.icon_size -= 16.;
        }
    }

    pub fn zoom_reset(&mut self) {
        self.icon_size = 64.;
    }

    fn icon_image_source(&self, dir_ent: &DirEntry, mime: &str, window: &Window, cx: &App) -> ImageSource {
        let app_global = cx.global::<AppGlobal>();
        if dir_ent.file_type().map(|file_type| file_type.is_dir()).unwrap_or(false) {
            app_global.match_directory_icon(self.icon_size as usize, window.scale_factor())
        } else {
            app_global.match_file_icon(mime, self.icon_size as usize, window.scale_factor())
        }
    }

    fn mime_type(&self, dir_ent: &DirEntry, cx: &App) -> String {
        let app_global = cx.global::<AppGlobal>();

        app_global.match_mime_type(dir_ent.file_name().to_str().unwrap_or(""))
    }

    fn clear_text_offset_cache(&mut self, window: &Window, cx: &App) {
        self.text_offset_cache_scale = window.scale_factor();
        self.text_offset_cache.clear();
        self.text_offset_cache
            .resize(self.model.read(cx).entries.len(), None);
    }

    fn reset_status(&mut self, cx: &Context<Self>) {
        self.status_prompt = None;
        self.status_text =
            SharedString::from(format!("{} Items", self.model.read(cx).entries.len()));
    }

    pub fn on_navigate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_text_offset_cache(window, cx);
        let path = self.model.read(cx).dir_path.to_str().unwrap().to_owned();
        window.set_window_title(&path);
        self.line_edit.update(cx, |_, cx| { cx.emit(DismissEvent); });
    }

    pub fn popup_line_edit(&mut self, window: &mut Window, cx: &mut Context<Self>, prompt: Option<StatusPrompt>, existing_text: Option<String>) {
        self.status_prompt = prompt;
        if let Some(text) = existing_text {
            self.line_edit.update(cx, |model, cx| {
                let text: SharedString = text.into();
                let len = text.len();
                model.content = text;
                model.select_to(len, cx);
            });
        }
        cx.focus_view(&self.line_edit, window);
    }

    fn on_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.model.read(cx).start_with.is_empty() {
            self.popup_line_edit(window, cx, Some(StatusPrompt::Search), None);
        } else {
            self.model.update(cx, &DirModel::search_next);
        }
    }

    fn text_offset_for_item(&mut self, window: &Window, cx: &App, idx: usize) -> f32 {
        if self.text_offset_cache_scale != window.scale_factor() {
            self.clear_text_offset_cache(window, cx);
        }
        if let Some(text_offset) = self.text_offset_cache[idx] {
            return text_offset;
        }

        let text_radius = self.text_radius();
        let text_width = self.text_width() - text_radius * 2.;
        let font_size = px(self.font_size());
        let text_system = window.text_system();
        let text_style = window.text_style();
        let text = self.model.read(cx).entries[idx]
            .file_name()
            .into_string()
            .unwrap_or(FILENAME_FALLBACK.to_string());

        let runs = vec![TextRun {
            len: text.len(),
            font: text_style.font(),
            color: text_style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];


        let text_offset = {
            let line_layout = text_system.layout_line(&text, font_size, &runs, None);
            let layout_width = line_layout.width.to_f64() as f32;
            if text_width > layout_width {
                (text_width - layout_width) / 2.
            } else {
                0.
            }
        };

        self.text_offset_cache[idx] = Some(text_offset);
        return text_offset;
    }

    fn full_item_width(&self) -> f32 {
        self.text_width() + 2. * self.margin_size()
    }

    fn full_item_height(&self) -> f32 {
        self.icon_size + self.margin_size() * 2. + self.font_size() + self.text_radius() * 2.
    }

    fn items_per_line(&self, window: &mut Window) -> usize {
        (window.bounds().size.width.to_f64() as f32 / self.full_item_width()) as usize
    }

    pub fn update_model<Func>(&mut self, window: &mut Window, cx: &mut Context<Self>, func: Func)
    where
        Func:
            FnMut(&mut DirModel, &mut Context<DirModel>) + std::marker::Copy,
    {
        self.update_model_view(window, cx, func, |_, _, _| {});
    }

    pub fn update_model_view<Func, ViewFunc>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        func: Func,
        view_func: ViewFunc,
    ) where
        Func:
            FnMut(&mut DirModel, &mut Context<DirModel>) + std::marker::Copy,
        ViewFunc: FnMut(&mut Self, &mut Window, &mut Context<Self>) + std::marker::Copy,
    {
        self.model.update(cx, func.clone());
        view_func.clone()(self, window, cx);

        self.scroll_handle
            .scroll_to_item(self.model.read(cx).current.unwrap_or(0) / self.items_per_line(window), ScrollStrategy::Top);

        cx.notify();
    }

    pub fn update_view<ViewFunc>(&mut self, window: &mut Window, cx: &mut Context<Self>, view_func: ViewFunc)
    where
        ViewFunc: FnMut(&mut Self, &mut Window, &mut Context<Self>) + std::marker::Copy,
    {
        self.update_model_view(window, cx, |_, _| {}, view_func);
    }

    fn io_worker_refresh_callback(&mut self, window: &mut Window, cx: &mut Context<Self>, open_result: OpenDirResult) {
        self.model.update(cx, |model, _| model.refresh_with_result(open_result));
        self.on_navigate(window, cx);
    }

    fn io_worker_open_callback(&mut self, window: &mut Window, cx: &mut Context<Self>, open_result: OpenDirResult) {
        self.model.update(cx, |model, _| model.open_with_result(open_result));
        self.on_navigate(window, cx);
    }

    pub fn update_with_io_worker<T: Send + 'static, Callback>(
        &mut self, window: &mut Window, cx: &mut Context<Self>, worker_result: Result<IOWorker<T>, String>, callback: Callback)
    where Callback: FnOnce(&mut Self, &mut Window, &mut Context<Self>, T) + 'static {
        match worker_result {
            Err(err) => {
                self.dialog.update(cx, |dialog, cx| {
                    dialog.show_just_error(err.into(), window, cx);
                });
            },
            Ok(worker) => {
                self.dialog.update(cx, |dialog, cx| {
                    dialog.show(
                        DialogRequest::new(worker.desc.into(), vec![]),
                        None,
                        window,
                        cx);
                });
                cx.spawn_in(window, async move |this, cx: &mut AsyncWindowContext| {
                    loop {
                        let Ok(ui_req) = worker.ui.recv().await else {
                            break;
                        };
                        this.update_in(cx, |this, window, cx| {
                            let chan = worker.input.clone();
                            let s = cx.subscribe(&this.dialog, move |_this, _dialog, response: &DialogResponse, cx| {
                                let chan = chan.clone();
                                let res = response.clone();
                                cx.spawn(async move |_this, _cx| {
                                    chan.send(res).await.expect("Cannot send to IOWorker");
                                }).detach();
                            });
                            this.dialog.update(cx, |dialog, cx| {
                                dialog.show(ui_req, Some(s), window, cx);
                            });
                        }).unwrap();
                    }
                    match worker.result.await {
                        Ok(result) => {
                            this.update_in(cx, |this, window, cx| {
                                callback(this, window, cx, result);
                                this.dialog.update(cx, &Dialog::hide);
                            }).unwrap();
                        },
                        Err(err) => {
                            this.update_in(cx, |this, window, cx| {
                                this.dialog.update(cx, |dialog, cx| {
                                    dialog.show_just_error(err.into(), window, cx)
                                });
                            }).unwrap();
                        }
                    }
                }).detach();
            }
        }
    }
}

impl Render for FileListView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let per_line = self.items_per_line(window);
        let nr_items = self.model.read(cx).entries.len();
        let nr_line = (nr_items + per_line - 1) / per_line;

        // println!("nr_items {} per-line {} nr_line {} content height {}",
        //          nr_items, per_line, nr_line, nr_line as f32 * self.full_item_height());

        let content_height = nr_line as f32 * self.full_item_height();
        let list_height = window.bounds().size.height.0 - 22.; // status bar
        let scroll_handle_off = self.scroll_handle.0.borrow().base_handle.offset().y.0;

        let scroll_off = (scroll_handle_off.max(list_height - content_height) * -1.).max(0.) * list_height / content_height;
        let scroll_sz = list_height * (list_height / content_height).min(1.);
        // println!("off {} height {}", off.y.0, cx.bounds().size.height.0);

        let mut status_children = vec![div()
            .w(px(128.))
            .text_size(px(12.))
            .child(self.status_text.clone())];
        if let Some(prompt) = &self.status_prompt {
            status_children.insert(0, div().flex_auto().child(self.line_edit.clone()));
            status_children.insert(0, div().text_size(px(12.)).child(prompt.to_str()));
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0xffffff))
            .track_focus(&self.focus_handle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .child(
                        uniform_list(
                            "entries",
                            nr_line,
                            cx.processor(move |this, range: std::ops::Range<usize>, window, cx| {
                                let mut items = Vec::new();
                                // println!("rendering new line {} {}", &range.start, &range.end);
                                this.scroll_range = range.clone();

                                for lidx in range {
                                    let mut line = Vec::new();
                                    let last_in_line =
                                        std::cmp::min((lidx + 1) * per_line, nr_items);
                                    for id in lidx * per_line..last_in_line {
                                        let dir_ent = &this.model.read(cx).entries[id];
                                        let mime = this.mime_type(dir_ent, cx);

                                        line.push(DirEntryView::new(
                                            id,
                                            this.icon_image_source(dir_ent, &mime, window, cx),
                                            cx.entity().clone(),
                                            mime,
                                            this.model.clone(),
                                            this.text_offset_for_item(window, cx, id),
                                        ));
                                    }
                                    items.push(div().flex().flex_row().children(line));
                                }
                                // cx.notify();

                                items
                            }),
                        )
                        .track_scroll(self.scroll_handle.clone())
                        .flex_auto(),
                    )
                    .child(
                        div().w_0p5().child(
                            div()
                                .top(px(scroll_off))
                                .h(px(scroll_sz))
                                .bg(rgb(0x59cdff)),
                        ),
                    )
                    .size_full(),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .bg(rgb(0xefefef))
                    .children(status_children),
            )
            .child(self.dialog.clone())
            .on_action(cx.listener(|this: &mut Self, action: &MoveAction, window, cx| {
                match action {
                    MoveAction::Next => { this.update_model(window, cx, &DirModel::move_next); },
                    MoveAction::Prev => { this.update_model(window, cx, &DirModel::move_prev); },
                    MoveAction::Home => { this.update_model(window, cx, &DirModel::move_home); },
                    MoveAction::End => { this.update_model(window, cx, &DirModel::move_end); },
                }
            }))
            .on_action(cx.listener(|this: &mut Self, _: &ToggleMark, window, cx| {
                this.update_model(window, cx, &DirModel::toggle_mark);
            }))
            .on_action(cx.listener(|this: &mut Self, _: &ToggleHidden, window, cx| {
                this.update_model_view(window, cx, &DirModel::toggle_hidden, &FileListView::on_navigate);
            }))
            .on_action(cx.listener(|this: &mut Self, _: &Open, window, cx| {
                let should_open_dir = this.model.read(cx).should_open_dir();
                match should_open_dir {
                    Some(true) => {
                        let worker = this.model.update(cx, &DirModel::open_dir);
                        this.update_with_io_worker(window, cx, worker, &Self::io_worker_open_callback);
                    },
                    Some(false) => {
                        let worker = this.model.update(cx, &DirModel::open_file);
                        this.update_with_io_worker(window, cx, worker, |this, _window, cx, open_result| {
                            this.model.update(cx, |_, cx| DirModel::after_open_file_result(open_result, cx));
                        });
                    },
                    None => {

                    },
                }
            }))
            .on_action(cx.listener(|this: &mut Self, action: &CopyOrCut, _window, cx| {
                this.model.update(cx, |model, cx| model.copy_or_move(cx, action.should_move));
            }))
            .on_action(cx.listener(|this: &mut Self, _: &Paste, window, cx| {
                let worker = this.model.update(cx, &DirModel::paste);
                this.update_with_io_worker(window, cx, worker, &Self::io_worker_refresh_callback);
            }))
            .on_action(cx.listener(move |this: &mut Self, _: &Remove, window, cx| {
                let worker = this.model.update(cx, &DirModel::delete);
                this.update_with_io_worker(window, cx, worker, &Self::io_worker_refresh_callback);
            }))
            .on_action(cx.listener(|this: &mut Self, _: &Up, window, cx| {
                let worker = this.model.update(cx, &DirModel::up);
                this.update_with_io_worker(window, cx, worker, &Self::io_worker_open_callback);
            }))
            .on_action(cx.listener(|this: &mut Self, _: &Back, window, cx| {
                if this.model.read(cx).start_with.is_empty() {
                    let worker = this.model.update(cx, &DirModel::back);
                    this.update_with_io_worker(window, cx, worker, |this, window, cx, open_result| {
                        this.model.update(cx, |model, _| model.back_with_result(open_result));
                        this.on_navigate(window, cx);
                    });
                } else {
                    this.update_model_view(
                        window,
                        cx,
                        &DirModel::search_clear,
                        &FileListView::on_search,
                    );
                }
            }))
            .on_action(cx.listener(|this: &mut Self, _: &Search, window, cx| {
                this.update_view(window, cx, &FileListView::on_search);
            }))
            .on_action(cx.listener(|this: &mut Self, _: &Rename, window, cx| {
                let Some(cur) = this.model.read(cx).current else {
                    return;
                };
                let existing_text = this.model.read(cx).entries[cur].file_name().to_string_lossy().to_string();

                this.update_view(window, cx, |this, window, cx| {
                    this.popup_line_edit(window, cx, Some(StatusPrompt::Rename), Some(existing_text.clone()));
                });
            }))
            .on_action(cx.listener(|this: &mut Self, _: &Escape, _window, cx| {
                // TODO: clear other UI modes too.
                this.line_edit.update(cx, |_, cx| cx.emit(DismissEvent));
            }))
            .on_action(cx.listener(|this: &mut Self,  _: &NewWindow, _window, cx| {
                let dir_path = this.model.read(cx).dir_path.clone();
                cx.spawn(async |_, cx: &mut AsyncApp| {
                    AppGlobal::new_main_window(dir_path, cx);
                }).detach();
            }))
            .on_action(cx.listener(|_: &mut Self, _: &CloseWindow, window, cx| {
                let should_quit = cx.windows().len() == 1;
                window.remove_window();
                if should_quit {
                    cx.quit();
                }
            }))
            .on_action(cx.listener(|this, action: &ZoomAction, window, cx| {
                match action {
                    ZoomAction::In => { this.zoom_in(); },
                    ZoomAction::Out => { this.zoom_out(); },
                    ZoomAction::Reset => { this.zoom_reset(); },
                }
                this.clear_text_offset_cache(window, cx);
                cx.notify();
            }))
    }
}

impl Focusable for FileListView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        println!("main get focus_handle");
        self.focus_handle.clone()
    }
}
