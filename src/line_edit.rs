use std::ops::Range;

use gpui::*;
use unicode_segmentation::*;

// Actions

#[derive(Clone, PartialEq, serde_derive::Deserialize, schemars::JsonSchema, Action)]
struct Move {
    forward: bool,
    word: bool,
    delete: bool,
}

impl Move {
    fn delete_action() -> Self { Self { forward: true, word: false, delete: true } }
    fn left_action() -> Self { Move { forward: false, word: false, delete: false } }
    fn right_action () -> Self { Move { forward: true, word: false, delete: false } }
    fn left_word_action () -> Self { Move { forward: false, word: true, delete: false } }
    fn right_word_action () -> Self { Move { forward: true, word: true, delete: false } }
}

actions!(
    text_input,
    [
        SelectAll,
        Home,
        End,
        StartSelection,
        ShowCharacterPalette,
        Cancel,
        Commit,
    ]
);

pub struct LineEdit {
    focus_handle: FocusHandle,
    pub content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
}

pub struct CommitEvent;

impl EventEmitter<DismissEvent> for LineEdit {}
impl EventEmitter<CommitEvent> for LineEdit {}

impl LineEdit {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        cx.on_focus(&focus_handle, window, |_, _window, cx: &mut Context<Self>| {
            println!("focus lineedit");
            cx.clear_key_bindings();

            cx.bind_keys([
                KeyBinding::new("backspace", Move { forward: false, word: false, delete: true }, None),
                KeyBinding::new("alt-backspace", Move { forward: false, word: true, delete: true }, None),
                KeyBinding::new("delete", Move::delete_action(), None),
                KeyBinding::new("ctrl-d", Move::delete_action(), None),
                KeyBinding::new("alt-d", Move { forward: true, word: true, delete: true }, None),
                KeyBinding::new("left", Move::left_action(), None),
                KeyBinding::new("ctrl-b", Move::left_action(), None),
                KeyBinding::new("alt-left", Move::left_word_action(), None),
                KeyBinding::new("alt-b", Move::left_word_action(), None),
                KeyBinding::new("right", Move::right_action(), None),
                KeyBinding::new("ctrl-f", Move::right_action(), None),
                KeyBinding::new("alt-right", Move::right_word_action(), None),
                KeyBinding::new("alt-f", Move::right_word_action(), None),
                KeyBinding::new("ctrl-x h", SelectAll, None),
                KeyBinding::new("home", Home, None),
                KeyBinding::new("ctrl-a", Home, None),
                KeyBinding::new("end", End, None),
                KeyBinding::new("ctrl-e", End, None),
                KeyBinding::new("escape", Cancel, None),
                KeyBinding::new("ctrl-g", Cancel, None),
                KeyBinding::new("ctrl-space", StartSelection, None),
                KeyBinding::new("enter", Commit, None),

                KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, None),
            ]);
        })
        .detach();

        LineEdit {
            focus_handle,
            content: "".into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
        }
    }

    pub fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn action_move(&mut self, action: &Move, window: &mut Window, cx: &mut Context<Self>) {
        if !action.delete || self.selected_range.is_empty() {
            let pos = if action.word {
                if action.forward {
                    Self::next_boundary(self.content.unicode_word_indices(), self.cursor_offset(), self.content.len())
                } else {
                    Self::prev_boundary(self.content.unicode_word_indices(), self.cursor_offset())
                }
            } else {
                if action.forward {
                    Self::next_boundary(self.content.grapheme_indices(true), self.cursor_offset(), self.content.len())
                } else {
                    Self::prev_boundary(self.content.grapheme_indices(true), self.cursor_offset())
                }
            };
            if self.is_selecting || action.delete {
                self.select_to(pos, cx);
            } else {
                self.move_to(pos, window, cx);
            }
        }

        if action.delete {
            self.replace_text_in_range(None, "", window, cx);
            self.is_selecting = false;
        }
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, window, cx);
        self.select_to(self.content.len(), cx)
    }

    fn start_selection(&mut self, _: &StartSelection, window: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        self.move_to(self.cursor_offset(), window, cx);
    }

    fn cancel(&mut self, _: &Cancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.is_selecting = false;
            self.move_to(self.cursor_offset(), window, cx);
            return;
        }
        cx.emit(DismissEvent);
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(0, cx);
        } else {
            self.move_to(0, window, cx);
        }
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        let end = self.content.len();
        if self.is_selecting {
            self.select_to(end, cx);
        } else {
            self.move_to(end, window, cx);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;

        if event.modifiers.shift {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        } else {
            self.move_to(self.index_for_mouse_position(event.position), window, cx)
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn show_character_palette(&mut self, _: &ShowCharacterPalette, window: &mut Window, _cx: &mut Context<Self>) {
        window.show_character_palette();
    }

    pub fn move_to(&mut self, offset: usize, _window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        cx.notify()
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };
        if position.y < bounds.top() {
            return 0;
        }
        if position.y > bounds.bottom() {
            return self.content.len();
        }
        line.closest_index_for_x(position.x - bounds.left())
    }

    pub fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset
        } else {
            self.selected_range.end = offset
        };
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
        cx.notify()
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    fn extra_seg_pattern(s: char) -> bool {
        s == '.' || s == '_'
    }

    fn prev_boundary<'a, I>(index: I, offset: usize) -> usize
    where I: DoubleEndedIterator<Item = (usize, &'a str)> {
        index.rev().find_map(|(idx, seg)| {
            for (didx, _) in seg.match_indices(Self::extra_seg_pattern).rev() {
                let p = didx + idx;
                if p + 1 < offset {
                    return Some(p + 1);
                }
            }
            (idx < offset).then_some(idx)
        }).unwrap_or(0)
    }

    fn next_boundary<'a, I>(mut index: I, offset: usize, limit: usize) -> usize
    where I: DoubleEndedIterator<Item = (usize, &'a str)> {
        index.find_map(|(idx, seg)| {
            for (didx, _) in seg.match_indices(Self::extra_seg_pattern) {
                let p = didx + idx;
                if p > offset {
                    return Some(p);
                }
            }
            (idx > offset).then_some(idx)
        }).unwrap_or(limit)
    }

    pub fn reset(&mut self) {
        self.content = "".into();
        self.selected_range = 0..0;
        self.selection_reversed = false;
        self.marked_range = None;
        self.last_layout = None;
        self.last_bounds = None;
        self.is_selecting = false;
    }
}

impl EntityInputHandler for LineEdit {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>
    ) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.selected_range = range.start + new_text.len()..range.start + new_text.len();
        self.marked_range.take();
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        self.content =
            (self.content[0..range.start].to_owned() + new_text + &self.content[range.end..])
                .into();
        self.marked_range = Some(range.start..range.start + new_text.len());
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.end)
            .unwrap_or_else(|| range.start + new_text.len()..range.start + new_text.len());

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);
        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start),
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end),
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;

        assert_eq!(last_layout.text, self.content);
        let utf8_index = last_layout.index_for_x(point.x - line_point.x)?;
        Some(self.offset_to_utf16(utf8_index))
    }

}

struct TextElement {
    input: Entity<LineEdit>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();

    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = (content.clone(), style.color);

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.marked_range.as_ref() {
            vec![
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run.clone()
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let cursor_pos = line.x_for_index(cursor);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() + cursor_pos, bounds.top()),
                        size(px(2.), bounds.bottom() - bounds.top()),
                    ),
                    gpui::blue(),
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    rgb(0xd3e3fd),
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection)
        }
        let line = prepaint.line.take().unwrap();
        line.paint(bounds.origin, window.line_height(), window, cx).unwrap();

        if focus_handle.is_focused(window) {
            if let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

impl Render for LineEdit {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .key_context("LineEdit")
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::action_move))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::start_selection))
            .on_action(cx.listener(Self::cancel))
            .on_action(cx.listener(|_, _: &Commit, _window, cx| cx.emit(CommitEvent)))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .bg(rgb(0xeeeeee))
            .text_size(px(12.))
            .child(
                div()
                    .h(px(20.))
                    .w_full()
                    .border_1()
                    .border_color(rgb(if self.focus_handle.is_focused(window) {0x59cdff} else {0xefefef}))
                    .bg(white())
                    .child(TextElement {
                        input: cx.entity().clone(),
                    }),
            )
    }
}

impl Focusable for LineEdit {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
