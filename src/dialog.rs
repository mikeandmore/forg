use std::path::PathBuf;

use gpui::*;
use crate::{app_global::AppGlobal, models::{DialogAction, DialogOption, DialogRequest, DialogResponse}};

pub struct Dialog {
    focus_handle: FocusHandle,
    visible: bool,
    msg: SharedString,
    actions: Vec<DialogAction>,
    pending: Option<Subscription>,
    options: Vec<DialogOption>,
    sel_option: Option<usize>,
}

actions!(dialog, [DialogNextOption, DialogPrevOption]);

impl EventEmitter<DialogResponse> for Dialog {}
impl EventEmitter<DismissEvent> for Dialog {}

impl Dialog {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            visible: false,
            msg: "".into(),
            actions: Vec::new(),
            pending: None,
            options: Vec::new(),
            sel_option: None,
        }
    }

    pub fn show(&mut self, request: DialogRequest, subscription: Option<Subscription>, window: &mut Window, cx: &mut Context<Self>) {
        self.msg = request.msg;
        self.visible = true;
        self.pending = subscription;
        self.actions = request.actions;
        self.options = request.options;
        self.sel_option = request.sel_option;

        println!("show dialog");
        // cx.on_focus(&self.focus_handle, |this, cx| {
        //     println!("Rebinding keys for dialog-mode");
        //     cx.clear_key_bindings();
        //     cx.window_context().bind_keys(this.actions.iter().enumerate().map(|(idx, a)| {
        //         KeyBinding::new(&a.key, DialogResponse(idx), None)
        //     }));
        // }).detach();
        self.bind_keys(window, cx);
        window.focus(&self.focus_handle);
        cx.notify();
    }

    fn bind_keys(&mut self, _window: &Window, cx: &mut Context<Self>) {
        println!("Rebinding keys for dialog-mode");
        cx.clear_key_bindings();
        cx.bind_keys(self.actions.iter().enumerate().map(|(idx, a)| {
            KeyBinding::new(&a.key, DialogResponse::new(idx, None), None)
        }));

        if !self.options.is_empty() {
            cx.bind_keys([
                KeyBinding::new("n", DialogNextOption, None),
                KeyBinding::new("p", DialogPrevOption, None),
            ]);
        }
    }

    pub fn show_just_error(&mut self, msg: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.show(
            DialogRequest::new(msg, vec![DialogAction::new("OK", "enter")]),
            None,
            window,
            cx);
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        drop(self.pending.take());
        cx.emit(DismissEvent);
        cx.notify();
    }
}

impl Render for Dialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible {
            return div().track_focus(&self.focus_handle);
        }

        let height = if self.options.is_empty() {
            200.
        } else {
            300.
        };

        let mut content = div().w_full().h(px(height)).bg(rgb(0xe5e2dc)).py_2().px_6().flex().flex_col();

        if !self.options.is_empty() {
            println!("opts len {}", self.options.len());
            content = content.child(
                div().flex().child(self.msg.clone())
            );
            content = content.child(uniform_list(
                "dialog_options",
                self.options.len(),
                cx.processor(|this, range: std::ops::Range<usize>, window, cx| {
                    let mut items = Vec::new();
                    let app_global = cx.global::<AppGlobal>();
                    for idx in range {
                        let img_src = app_global.match_icon(this.options[idx].icon_name.as_str(), 32, window.scale_factor())
                            .unwrap_or(PathBuf::from("").into());
                        let mut item = div().flex().flex_row().w_full().h(px(32.)).child(
                            img(img_src).h(px(32.)).w(px(32.))
                        ).child(this.options[idx].text.clone());
                        if Some(idx) == this.sel_option {
                            item = item.bg(rgb(0x0068d9));
                        }
                        items.push(item);
                    }
                    items
                })).bg(rgb(0xffffff)).flex_grow());
        } else {
            content = content.child(
                div().flex_grow().child(self.msg.clone())
            );
        }

        content = content.child(div().flex().flex_row().justify_center().children(self.actions.iter().enumerate().map(|(idx, action)| {
            div().border_1().border_color(rgb(0x787878)).cursor_pointer()
                .px_2().m_1()
                .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, _, cx| cx.emit(DialogResponse::new(idx, this.sel_option.clone()))))
                .child(format!("{} [{}]", action.text, action.key))
        })));

        let mut d = div().absolute().size_full().bg(rgba(0xeeeeee77)).px_8().flex().justify_center().child(content).track_focus(&self.focus_handle);
        d = d.on_action(cx.listener(|this, a: &DialogResponse, _, cx| {
            if this.pending.is_some() {
                cx.emit(DialogResponse {
                    action: a.action,
                    sel_option: this.sel_option.clone(),
                });
            } else {
                this.hide(cx);
            }
        }));

        if !self.options.is_empty() {
            d = d.on_action(cx.listener(|this, _: &DialogNextOption, _, cx| {
                this.sel_option = Some(this.sel_option.map(|sel| std::cmp::min(sel + 1, this.options.len() - 1)).unwrap_or(0));
                cx.notify();
            }));
            d = d.on_action(cx.listener(|this, _: &DialogPrevOption, _, cx| {
                this.sel_option = this.sel_option.and_then(|sel| if sel == 0 { None } else { Some(sel - 1) });
                cx.notify();
            }));
        }

        d
    }
}
