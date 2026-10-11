//! The text editor view: selections, multi-cursor, rectangular
//! selection, folding, bookmarks, completion and rendering.

pub mod display;
mod editing;
pub mod element;
mod input;
mod movement;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    div, font, prelude::*, px, Action, App, Context, Entity, EventEmitter, FocusHandle, Focusable, Font,
    Global, KeyContext, Pixels, Point, SharedString, ShapedLine, Subscription, TextRun, Window,
};

use crate::buffer::{Buffer, BufferEvent, EditInfo, EditKind, Selection};
use crate::settings::Settings;
use crate::syntax::Span;
use crate::theme::AppTheme;
use display::{DisplayLine, DisplayMap, SpecialKind};
pub use element::EditorElement;
pub use editing::{push_clipboard_history, ClipboardHistory};
pub use movement::word_at as word_at_pub;

/// Defines editor actions and generates registration + dynamic dispatch (used by macro playback
/// and by the workspace to forward menu commands to the active document).
macro_rules! editor_action_group {
    ($register:ident, $dispatch:ident, $names:ident, [$($action:ident => $method:ident),* $(,)?]) => {
        gpui::actions!(editor, [$($action),*]);

        impl Editor {
            pub fn $register<E: InteractiveElement>(el: E, cx: &mut Context<Self>) -> E {
                el $(.on_action(cx.listener(|this: &mut Editor, a: &$action, window, cx| {
                    this.record_step(MacroStep::Action(Box::new(a.clone())), cx);
                    this.$method(window, cx);
                })))*
            }

            pub fn $dispatch(&mut self, action: &dyn Action, window: &mut Window, cx: &mut Context<Self>) -> bool {
                let any = action.as_any();
                $(if any.is::<$action>() {
                    self.$method(window, cx);
                    return true;
                })*
                false
            }
        }

        pub fn $names() -> Vec<Box<dyn Action>> {
            vec![$(Box::new($action)),*]
        }
    };
}

editor_action_group!(register_basic_actions, dispatch_basic_action, basic_actions, [
    MoveLeft => move_left,
    MoveRight => move_right,
    MoveUp => move_up,
    MoveDown => move_down,
    MoveWordLeft => move_word_left,
    MoveWordRight => move_word_right,
    MoveToLineStart => move_to_line_start,
    MoveToLineEnd => move_to_line_end,
    MoveToDocStart => move_to_doc_start,
    MoveToDocEnd => move_to_doc_end,
    PageUp => page_up,
    PageDown => page_down,
    SelectLeft => select_left,
    SelectRight => select_right,
    SelectUp => select_up,
    SelectDown => select_down,
    SelectWordLeft => select_word_left,
    SelectWordRight => select_word_right,
    SelectToLineStart => select_to_line_start,
    SelectToLineEnd => select_to_line_end,
    SelectToDocStart => select_to_doc_start,
    SelectToDocEnd => select_to_doc_end,
    SelectPageUp => select_page_up,
    SelectPageDown => select_page_down,
    Backspace => backspace,
    Delete => delete,
    DeleteWordLeft => delete_word_left,
    DeleteWordRight => delete_word_right,
    DeleteToLineStart => delete_to_line_start,
    DeleteToLineEnd => delete_to_line_end,
    Newline => newline,
    Tab => tab,
    ShiftTab => shift_tab,
    Undo => undo,
    Redo => redo,
    Cut => cut,
    Copy => copy,
    Paste => paste,
    SelectAll => select_all,
    Cancel => cancel,
]);

editor_action_group!(register_doc_actions, dispatch_doc_action, doc_actions, [
    ColumnSelectUp => column_select_up,
    ColumnSelectDown => column_select_down,
    ColumnSelectLeft => column_select_left,
    ColumnSelectRight => column_select_right,
    AddCursorAbove => add_cursor_above,
    AddCursorBelow => add_cursor_below,
    ToggleSelectionAnchor => toggle_selection_anchor,
    ToggleColumnSelectionAnchor => toggle_column_selection_anchor,
    SelectLine => select_line,
    SelectWord => select_word,
    SelectNextOccurrence => select_next_occurrence,
    NewlineAbove => newline_above,
    NewlineBelow => newline_below,
    Indent => indent,
    Outdent => outdent,
    DuplicateLine => duplicate_line,
    DeleteLine => delete_line,
    MoveLineUp => move_line_up,
    MoveLineDown => move_line_down,
    JoinLines => join_lines,
    SplitLines => split_lines,
    TransposeLines => transpose_lines,
    ToggleOverwrite => toggle_overwrite,
    UpperCase => upper_case,
    LowerCase => lower_case,
    TitleCase => title_case,
    TitleCaseKeepCaps => title_case_keep_caps,
    SentenceCase => sentence_case,
    SentenceCaseKeepCaps => sentence_case_keep_caps,
    InvertCase => invert_case,
    RandomCase => random_case,
    RemoveDuplicateLines => remove_duplicate_lines,
    RemoveAdjacentDuplicateLines => remove_adjacent_duplicate_lines,
    RemoveEmptyLines => remove_empty_lines,
    RemoveBlankLines => remove_blank_lines,
    InsertBlankLineAbove => insert_blank_line_above,
    InsertBlankLineBelow => insert_blank_line_below,
    SortLinesLexAsc => sort_lines_lex_asc,
    SortLinesLexDesc => sort_lines_lex_desc,
    SortLinesLexCiAsc => sort_lines_lex_ci_asc,
    SortLinesLexCiDesc => sort_lines_lex_ci_desc,
    SortLinesIntAsc => sort_lines_int_asc,
    SortLinesIntDesc => sort_lines_int_desc,
    SortLinesDecimalDotAsc => sort_lines_decimal_dot_asc,
    SortLinesDecimalDotDesc => sort_lines_decimal_dot_desc,
    SortLinesDecimalCommaAsc => sort_lines_decimal_comma_asc,
    SortLinesDecimalCommaDesc => sort_lines_decimal_comma_desc,
    SortLinesLengthAsc => sort_lines_length_asc,
    SortLinesLengthDesc => sort_lines_length_desc,
    ReverseLines => reverse_lines,
    RandomizeLines => randomize_lines,
    ToggleLineComment => toggle_line_comment,
    LineComment => line_comment,
    LineUncomment => line_uncomment,
    BlockComment => block_comment,
    BlockUncomment => block_uncomment,
    TrimTrailingSpace => trim_trailing_space,
    TrimLeadingSpace => trim_leading_space,
    TrimLeadingAndTrailingSpace => trim_leading_and_trailing_space,
    JoinLinesWithSpaces => join_lines_with_spaces,
    TrimAndJoinLines => trim_and_join_lines,
    TabsToSpaces => tabs_to_spaces,
    SpacesToTabsAll => spaces_to_tabs_all,
    SpacesToTabsLeading => spaces_to_tabs_leading,
    InsertDateTimeShort => insert_date_time_short,
    InsertDateTimeLong => insert_date_time_long,
    InsertDateTimeCustom => insert_date_time_custom,
    GoToMatchingBrace => go_to_matching_brace,
    SelectToMatchingBrace => select_to_matching_brace,
    ToggleBookmark => toggle_bookmark,
    NextBookmark => next_bookmark,
    PrevBookmark => prev_bookmark,
    ClearBookmarks => clear_bookmarks,
    CutBookmarkedLines => cut_bookmarked_lines,
    CopyBookmarkedLines => copy_bookmarked_lines,
    ReplaceBookmarkedLines => replace_bookmarked_lines,
    RemoveBookmarkedLines => remove_bookmarked_lines,
    KeepOnlyBookmarkedLines => keep_only_bookmarked_lines,
    InvertBookmarks => invert_bookmarks,
    FoldAll => fold_all,
    UnfoldAll => unfold_all,
    FoldCurrent => fold_current,
    UnfoldCurrent => unfold_current,
    ToggleFold => toggle_fold_at_caret,
    FoldLevel1 => fold_level_1,
    FoldLevel2 => fold_level_2,
    FoldLevel3 => fold_level_3,
    FoldLevel4 => fold_level_4,
    FoldLevel5 => fold_level_5,
    FoldLevel6 => fold_level_6,
    FoldLevel7 => fold_level_7,
    FoldLevel8 => fold_level_8,
    UnfoldLevel1 => unfold_level_1,
    UnfoldLevel2 => unfold_level_2,
    UnfoldLevel3 => unfold_level_3,
    UnfoldLevel4 => unfold_level_4,
    UnfoldLevel5 => unfold_level_5,
    UnfoldLevel6 => unfold_level_6,
    UnfoldLevel7 => unfold_level_7,
    UnfoldLevel8 => unfold_level_8,
    HideLines => hide_lines,
    UnhideAllLines => unhide_all_lines,
    HighlightAll1 => highlight_all_1,
    HighlightAll2 => highlight_all_2,
    HighlightAll3 => highlight_all_3,
    HighlightAll4 => highlight_all_4,
    HighlightAll5 => highlight_all_5,
    HighlightOne1 => highlight_one_1,
    HighlightOne2 => highlight_one_2,
    HighlightOne3 => highlight_one_3,
    HighlightOne4 => highlight_one_4,
    HighlightOne5 => highlight_one_5,
    ClearHighlight1 => clear_highlight_1,
    ClearHighlight2 => clear_highlight_2,
    ClearHighlight3 => clear_highlight_3,
    ClearHighlight4 => clear_highlight_4,
    ClearHighlight5 => clear_highlight_5,
    ClearAllHighlights => clear_all_highlights,
    ClearFindMarks => clear_find_marks,
    JumpToNextMark => jump_to_next_mark,
    JumpToPrevMark => jump_to_prev_mark,
    CopyHighlightedText => copy_highlighted_text,
    TriggerCompletion => trigger_completion,
    CompletionNext => completion_next,
    CompletionPrev => completion_prev,
    CompletionConfirm => completion_confirm,
    ScrollLineUp => scroll_line_up,
    ScrollLineDown => scroll_line_down,
    Base64Encode => base64_encode,
    Base64Decode => base64_decode,
    UrlEncode => url_encode,
    UrlDecode => url_decode,
    AsciiToHex => ascii_to_hex,
    HexToAscii => hex_to_ascii,
    JsonFormat => json_format,
    JsonMinify => json_minify,
    XmlPrettyPrint => xml_pretty_print,
]);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditorMode {
    /// A document editor.
    Full,
    /// A one-line text field (find box, dialogs).
    SingleLine,
    /// A small multi-line text field inside dialogs.
    MultiLine,
}

#[derive(Clone, Debug)]
pub enum EditorEvent {
    SelectionsChanged,
    Edited,
    Confirm { shift: bool },
    Cancel,
    Focused,
    Blurred,
    ContextMenu(Point<Pixels>),
    Zoom(i32),
    Scrolled,
    /// A message for the status bar / user (e.g. "Base64 decode failed").
    Notify(String),
}

impl EventEmitter<EditorEvent> for Editor {}

pub enum MacroStep {
    Action(Box<dyn Action>),
    Insert(String),
}

impl Clone for MacroStep {
    fn clone(&self) -> Self {
        match self {
            MacroStep::Action(a) => MacroStep::Action(a.boxed_clone()),
            MacroStep::Insert(s) => MacroStep::Insert(s.clone()),
        }
    }
}

#[derive(Default)]
pub struct MacroRecorder {
    pub recording: bool,
    pub steps: Vec<MacroStep>,
    pub last: Vec<MacroStep>,
}

impl Global for MacroRecorder {}

#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(dead_code)]
pub enum Autoscroll {
    Fit,
    Center,
    Top,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragMode {
    Char,
    Word,
    Line,
    Column,
    VScrollbar { grab: f32 },
    HScrollbar { grab: f32 },
}

#[derive(Clone, Debug)]
pub struct DragState {
    pub mode: DragMode,
    /// The selection created on mouse down (word / line granularity extends from it).
    pub origin: Selection,
    pub last_pos: Point<Pixels>,
    /// Index of the selection being dragged (multi-cursor add with cmd+click).
    pub sel_index: usize,
}

/// Rectangular (column) selection described by display x positions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectSelection {
    pub anchor_line: usize,
    pub anchor_x: f32,
    pub head_line: usize,
    pub head_x: f32,
}

pub struct Completion {
    pub items: Vec<String>,
    pub selected: usize,
    pub word_start: usize,
}

pub struct EditorStyle {
    pub font: Font,
    pub font_size: Pixels,
    pub line_height: Pixels,
}

/// One shaped display row (a whole line, or one wrapped segment of it).
#[derive(Clone)]
pub struct RowSeg {
    pub disp: Range<usize>,
    /// Line-relative char range covered by this row.
    pub chars: Range<usize>,
    pub shaped: ShapedLine,
}

#[derive(Clone)]
pub struct LineShape {
    pub line: usize,
    pub start_char: usize,
    pub len_chars: usize,
    pub dl: Rc<DisplayLine>,
    pub segs: Vec<RowSeg>,
}

impl LineShape {
    /// Which segment contains a line-relative char index.
    pub fn seg_for_char(&self, col: usize) -> usize {
        for (i, s) in self.segs.iter().enumerate() {
            if col < s.chars.end || i + 1 == self.segs.len() {
                return i;
            }
        }
        0
    }

    pub fn x_for_char(&self, col: usize) -> (usize, f32) {
        let si = self.seg_for_char(col);
        let seg = &self.segs[si];
        let disp = self.dl.char_to_disp[col.min(self.dl.char_count())] as usize;
        let local = disp.saturating_sub(seg.disp.start).min(seg.disp.end - seg.disp.start);
        (si, f32::from(seg.shaped.x_for_index(local)))
    }

    pub fn char_for_x(&self, seg_index: usize, x: f32) -> usize {
        let seg = &self.segs[seg_index.min(self.segs.len() - 1)];
        let local = seg.shaped.closest_index_for_x(px(x.max(0.0)));
        let col = self.dl.disp_to_char(seg.disp.start + local);
        let max = if seg_index + 1 < self.segs.len() {
            // Do not land on the first char of the next row.
            seg.chars.end.saturating_sub(1).max(seg.chars.start)
        } else {
            seg.chars.end
        };
        col.clamp(seg.chars.start, max.max(seg.chars.start))
    }
}

/// Geometry from the last paint, used for hit testing and IME.
#[derive(Clone)]
#[allow(dead_code)]
pub struct LayoutSnapshot {
    pub bounds: gpui::Bounds<Pixels>,
    pub text_bounds: gpui::Bounds<Pixels>,
    pub gutter: gpui::Bounds<Pixels>,
    pub line_numbers_x: Range<f32>,
    pub bookmark_x: Range<f32>,
    pub fold_x: Range<f32>,
    pub line_height: f32,
    pub space_width: f32,
    pub first_row: usize,
    pub rows: Vec<RowInfo>,
    pub row_count: usize,
    pub vscroll: Option<ScrollbarInfo>,
    pub hscroll: Option<ScrollbarInfo>,
    pub content_width: f32,
}

#[derive(Clone)]
pub struct RowInfo {
    pub row: usize,
    pub line: usize,
    pub sub: usize,
    pub y: f32,
    pub shape: Rc<LineShape>,
}

#[derive(Clone, Copy, Debug)]
pub struct ScrollbarInfo {
    pub track: gpui::Bounds<Pixels>,
    pub thumb: gpui::Bounds<Pixels>,
    pub max_scroll: f32,
}

pub struct Editor {
    pub view_id: u64,
    pub buffer: Entity<Buffer>,
    pub mode: EditorMode,
    pub focus_handle: FocusHandle,
    pub selections: Vec<Selection>,
    pub primary: usize,
    pub rect: Option<RectSelection>,
    pub selection_anchor: Option<(usize, bool)>,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub overwrite: bool,
    pub folds: BTreeMap<usize, usize>,
    pub display: DisplayMap,
    pub layout: Option<LayoutSnapshot>,
    pub(crate) marked_range: Option<Range<usize>>,
    pub(crate) drag: Option<DragState>,
    pub(crate) cursor_visible: bool,
    blink_epoch: usize,
    pub(crate) autoscroll: Option<Autoscroll>,
    pub completion: Option<Completion>,
    pub placeholder: SharedString,
    pub max_content_width: f32,
    pub(crate) font_key: (SharedString, f32),
    pub incremental_highlight: Vec<Range<usize>>,
    pub focused: bool,
    pub(crate) last_scroll_emitted: (f32, f32),
    pub(crate) text_left: f32,
    pub(crate) y_offset: f32,
    pub(crate) zoom_accum: f32,
    #[allow(clippy::type_complexity)]
    pub(crate) tag_cache: Option<((u64, usize), Option<(Range<usize>, Option<Range<usize>>)>)>,
    _subscriptions: Vec<Subscription>,
}

static NEXT_VIEW_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Editor {
    pub fn new(buffer: Entity<Buffer>, mode: EditorMode, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let sub = cx.subscribe(&buffer, Self::on_buffer_event);
        let focus_in = cx.on_focus(&focus_handle, window, |this: &mut Editor, _window, cx| {
            this.focused = true;
            this.restart_blink(cx);
            cx.emit(EditorEvent::Focused);
        });
        let focus_out = cx.on_blur(&focus_handle, window, |this: &mut Editor, _window, cx| {
            this.focused = false;
            this.completion = None;
            cx.emit(EditorEvent::Blurred);
            cx.notify();
        });
        Editor {
            view_id: NEXT_VIEW_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            buffer,
            mode,
            focus_handle,
            selections: vec![Selection::caret(0)],
            primary: 0,
            rect: None,
            selection_anchor: None,
            scroll_x: 0.0,
            scroll_y: 0.0,
            overwrite: false,
            folds: BTreeMap::new(),
            display: DisplayMap::new(),
            layout: None,
            marked_range: None,
            drag: None,
            cursor_visible: true,
            blink_epoch: 0,
            autoscroll: None,
            completion: None,
            placeholder: SharedString::default(),
            max_content_width: 0.0,
            font_key: (SharedString::default(), 0.0),
            incremental_highlight: Vec::new(),
            focused: false,
            last_scroll_emitted: (0.0, 0.0),
            text_left: 0.0,
            y_offset: 0.0,
            zoom_accum: 0.0,
            tag_cache: None,
            _subscriptions: vec![sub, focus_in, focus_out],
        }
    }

    /// Create a single-line input with its own private buffer.
    pub fn single_line(placeholder: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|_| Buffer::new(0, String::new(), crate::encoding::EolMode::Lf, Default::default()));
        let mut e = Editor::new(buffer, EditorMode::SingleLine, window, cx);
        e.placeholder = placeholder.to_string().into();
        e
    }

    pub fn multi_line(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let buffer = cx.new(|_| Buffer::new(0, String::new(), crate::encoding::EolMode::Lf, Default::default()));
        Editor::new(buffer, EditorMode::MultiLine, window, cx)
    }

    // ------------------------------------------------------------------ basic accessors

    pub fn buf<'a>(&self, cx: &'a App) -> &'a Buffer {
        self.buffer.read(cx)
    }

    pub fn text(&self, cx: &App) -> String {
        self.buffer.read(cx).text()
    }

    pub fn set_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.buf(cx).len_chars();
        let text = if self.mode == EditorMode::SingleLine { text.replace(['\r', '\n'], " ") } else { text.to_string() };
        let origin = self.view_id;
        self.buffer.update(cx, |b, cx| {
            b.edit(vec![(0..len, text)], EditKind::Other, vec![], origin, cx);
        });
        let end = self.buf(cx).len_chars();
        self.set_selections(vec![Selection::new(0, end)], cx);
        let _ = window;
    }

    pub fn primary_selection(&self) -> Selection {
        self.selections.get(self.primary).copied().unwrap_or_default()
    }

    pub fn caret(&self) -> usize {
        self.primary_selection().head
    }

    pub fn selected_text(&self, cx: &App) -> String {
        let s = self.primary_selection();
        self.buf(cx).slice_string(s.range())
    }

    pub fn has_selection(&self) -> bool {
        self.selections.iter().any(|s| !s.is_empty())
    }

    pub fn style(&self, cx: &App) -> EditorStyle {
        let s = cx.global::<Settings>();
        match self.mode {
            EditorMode::Full => {
                let size = s.effective_font_size();
                EditorStyle {
                    font: font(SharedString::from(s.font_family.clone())),
                    font_size: px(size),
                    line_height: px((size * 1.34).round()),
                }
            }
            EditorMode::SingleLine | EditorMode::MultiLine => {
                let size = s.ui_font_size;
                EditorStyle { font: font(".SystemUIFont"), font_size: px(size), line_height: px((size * 1.45).round()) }
            }
        }
    }

    pub fn word_wrap(&self, cx: &App) -> bool {
        match self.mode {
            EditorMode::Full => cx.global::<Settings>().word_wrap,
            EditorMode::SingleLine => false,
            EditorMode::MultiLine => true,
        }
    }

    pub fn tab_size(&self, cx: &App) -> usize {
        cx.global::<Settings>().tab_size.max(1)
    }

    pub(crate) fn record_step(&self, step: MacroStep, cx: &mut Context<Self>) {
        if self.mode != EditorMode::Full {
            return;
        }
        if let Some(rec) = cx.try_global::<MacroRecorder>() {
            if rec.recording {
                cx.global_mut::<MacroRecorder>().steps.push(step);
            }
        }
    }

    // ------------------------------------------------------------------ selections

    /// Sort, merge overlaps, clamp, then store selections. `primary_pos` selects the primary by head.
    pub fn set_selections_with_primary(&mut self, sels: Vec<Selection>, primary_head: Option<usize>, cx: &mut Context<Self>) {
        let len = self.buf(cx).len_chars();
        let mut sels: Vec<Selection> = sels
            .into_iter()
            .map(|s| Selection { anchor: s.anchor.min(len), head: s.head.min(len), goal: s.goal })
            .collect();
        if sels.is_empty() {
            sels.push(Selection::caret(0));
        }
        let primary_head = primary_head.unwrap_or_else(|| sels.last().map(|s| s.head).unwrap_or(0));
        sels.sort_by_key(|s| (s.start(), s.end()));
        let mut merged: Vec<Selection> = Vec::with_capacity(sels.len());
        for s in sels {
            if let Some(last) = merged.last_mut() {
                let overlap = s.start() < last.end()
                    || s.start() == last.start()
                    || (s.start() == last.end() && (s.is_empty() || last.is_empty()));
                if overlap {
                    let start = last.start().min(s.start());
                    let end = last.end().max(s.end());
                    let rev = last.reversed();
                    *last = if rev { Selection::new(end, start) } else { Selection::new(start, end) };
                    continue;
                }
            }
            merged.push(s);
        }
        self.primary = merged
            .iter()
            .position(|s| s.head == primary_head || (s.start() <= primary_head && primary_head <= s.end()))
            .unwrap_or(merged.len() - 1);
        self.selections = merged;
        self.autoscroll = Some(self.autoscroll.unwrap_or(Autoscroll::Fit));
        self.completion_refilter(cx);
        self.restart_blink(cx);
        cx.emit(EditorEvent::SelectionsChanged);
        cx.notify();
    }

    pub fn set_selections(&mut self, sels: Vec<Selection>, cx: &mut Context<Self>) {
        let primary = sels.last().map(|s| s.head);
        self.set_selections_with_primary(sels, primary, cx);
    }

    /// Select a range and scroll it into view (centred), unfolding if needed.
    pub fn select_range(&mut self, range: Range<usize>, center: bool, cx: &mut Context<Self>) {
        self.rect = None;
        let line = self.buf(cx).char_to_line(range.start);
        self.ensure_line_visible(line, cx);
        self.set_selections(vec![Selection::new(range.start, range.end)], cx);
        self.autoscroll = Some(if center { Autoscroll::Center } else { Autoscroll::Fit });
    }

    pub fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let b = self.buf(cx);
        let line = line.min(b.len_lines().saturating_sub(1));
        let pos = b.line_to_char(line);
        self.select_range(pos..pos, true, cx);
    }

    #[allow(dead_code)]
    pub fn request_autoscroll(&mut self, a: Autoscroll, cx: &mut Context<Self>) {
        self.autoscroll = Some(a);
        cx.notify();
    }

    pub fn restart_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        self.blink_epoch += 1;
        let epoch = self.blink_epoch;
        if !self.focused || !cx.global::<Settings>().caret_blink {
            return;
        }
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(530)).await;
                let keep_going = this
                    .update(cx, |this, cx| {
                        if this.blink_epoch != epoch || !this.focused {
                            return false;
                        }
                        this.cursor_visible = !this.cursor_visible;
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();
    }

    // ------------------------------------------------------------------ buffer events

    fn on_buffer_event(&mut self, _buffer: Entity<Buffer>, event: &BufferEvent, cx: &mut Context<Self>) {
        match event {
            BufferEvent::Edited { infos, origin } => {
                self.display.apply_edits(infos);
                self.adjust_folds(infos);
                // Other views of the same buffer keep their carets in place.
                if *origin != self.view_id {
                    let sels: Vec<Selection> = self
                        .selections
                        .iter()
                        .map(|s| Selection {
                            anchor: crate::buffer::map_pos(s.anchor, infos, false),
                            head: crate::buffer::map_pos(s.head, infos, false),
                            goal: None,
                        })
                        .collect();
                    self.selections = sels;
                    let len = self.buf(cx).len_chars();
                    for s in self.selections.iter_mut() {
                        s.anchor = s.anchor.min(len);
                        s.head = s.head.min(len);
                    }
                }
                self.incremental_highlight.clear();
                cx.emit(EditorEvent::Edited);
                cx.notify();
            }
            BufferEvent::Reloaded => {
                self.display.invalidate();
                self.folds.clear();
                let len = self.buf(cx).len_chars();
                for s in self.selections.iter_mut() {
                    s.anchor = s.anchor.min(len);
                    s.head = s.head.min(len);
                }
                cx.notify();
            }
            BufferEvent::Saved | BufferEvent::MetaChanged => cx.notify(),
        }
    }

    fn adjust_folds(&mut self, infos: &[EditInfo]) {
        if self.folds.is_empty() {
            return;
        }
        let mut folds: Vec<(usize, usize)> = self.folds.iter().map(|(a, b)| (*a, *b)).collect();
        for e in infos.iter().rev() {
            let removed = e.old_end_line - e.start_line;
            let delta = e.new_line_count as isize - removed as isize;
            if removed == 0 && delta == 0 {
                continue;
            }
            let map = |l: usize| -> usize {
                if l <= e.start_line {
                    l
                } else if l <= e.old_end_line {
                    e.start_line
                } else {
                    (l as isize + delta).max(0) as usize
                }
            };
            for f in folds.iter_mut() {
                *f = (map(f.0), map(f.1));
            }
        }
        self.folds = folds.into_iter().filter(|(h, e)| e > h).collect();
        self.display.invalidate_folds();
    }

    /// Apply edits originating from this view. Returns edit infos in original coordinates.
    pub fn transact(&mut self, edits: Vec<(Range<usize>, String)>, kind: EditKind, cx: &mut Context<Self>) -> std::sync::Arc<Vec<EditInfo>> {
        if self.buf(cx).is_read_only() && self.mode == EditorMode::Full {
            cx.emit(EditorEvent::Notify("The document is read-only".into()));
            return std::sync::Arc::new(Vec::new());
        }
        let before = self.selections.clone();
        let origin = self.view_id;
        self.buffer.update(cx, |b, cx| b.edit(edits, kind, before, origin, cx))
    }

    pub fn finish_transaction(&mut self, cx: &mut Context<Self>) {
        let sels = self.selections.clone();
        self.buffer.update(cx, |b, _| b.set_selections_after(sels));
    }

    pub fn ensure_line_visible(&mut self, line: usize, cx: &mut Context<Self>) {
        let mut changed = false;
        let headers: Vec<usize> = self.folds.iter().filter(|(h, e)| **h < line && line <= **e).map(|(h, _)| *h).collect();
        for h in headers {
            self.folds.remove(&h);
            changed = true;
        }
        let hidden_hit = self.buf(cx).hidden_ranges.iter().any(|r| r.start <= line && line < r.end);
        if hidden_hit {
            self.buffer.update(cx, |b, _| b.hidden_ranges.retain(|r| !(r.start <= line && line < r.end)));
            changed = true;
        }
        if changed {
            self.display.invalidate_folds();
            cx.notify();
        }
    }

    // ------------------------------------------------------------------ shaping

    /// Build text runs for the display range `disp` of a line.
    pub fn build_runs(
        dl: &DisplayLine,
        spans: Option<&[Span]>,
        disp: Range<usize>,
        style: &EditorStyle,
        theme: &AppTheme,
        default_color: gpui::Hsla,
    ) -> Vec<TextRun> {
        // Styled pieces over the whole line: (disp_end, color, style_bits)
        let mut pieces: Vec<(usize, gpui::Hsla, u8)> = Vec::new();
        if let Some(spans) = spans {
            let mut ch = 0usize;
            for sp in spans {
                ch += sp.len as usize;
                let end = dl.char_to_disp[ch.min(dl.char_count())] as usize;
                pieces.push((end, crate::theme::rgb_hex(sp.color), sp.style));
            }
        }
        if pieces.last().map(|p| p.0).unwrap_or(0) < dl.text.len() {
            pieces.push((dl.text.len(), default_color, 0));
        }
        // Control characters are drawn inverted.
        let mut out: Vec<TextRun> = Vec::new();
        let mut pos = disp.start;
        let mut piece_start = 0usize;
        let mut specials = dl.specials.iter().filter(|(_, k)| *k == SpecialKind::Control).peekable();
        for (pend, color, bits) in pieces {
            let pstart = piece_start;
            piece_start = pend;
            if pend <= disp.start || pstart >= disp.end {
                continue;
            }
            let mut s = pstart.max(disp.start);
            let e = pend.min(disp.end);
            while s < e {
                // Split around control boxes.
                while let Some((r, _)) = specials.peek() {
                    if r.end <= s {
                        specials.next();
                    } else {
                        break;
                    }
                }
                let (seg_end, seg_color) = match specials.peek() {
                    Some((r, _)) if r.start <= s => (r.end.min(e), theme.ctrl_fg),
                    Some((r, _)) if r.start < e => (r.start, color),
                    _ => (e, color),
                };
                let mut f = style.font.clone();
                if bits & 1 != 0 {
                    f = f.bold();
                }
                if bits & 2 != 0 {
                    f = f.italic();
                }
                let underline = if bits & 4 != 0 {
                    Some(gpui::UnderlineStyle { thickness: px(1.0), color: Some(seg_color), wavy: false })
                } else {
                    None
                };
                out.push(TextRun {
                    len: seg_end - s,
                    font: f,
                    color: seg_color,
                    background_color: None,
                    underline,
                    strikethrough: None,
                });
                s = seg_end;
            }
            pos = e;
        }
        let _ = pos;
        if out.is_empty() {
            out.push(TextRun {
                len: disp.end - disp.start,
                font: style.font.clone(),
                color: default_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            });
        }
        out
    }

    /// Shape a buffer line into display rows.
    pub fn shape_line(&mut self, line: usize, window: &mut Window, cx: &mut Context<Self>) -> LineShape {
        let style = self.style(cx);
        let theme = crate::theme::theme(cx);
        let show_ws = self.mode == EditorMode::Full && cx.global::<Settings>().show_whitespace;
        let tab = self.tab_size(cx);
        let b = self.buffer.read(cx);
        let text = b.line_text(line);
        let start_char = b.line_to_char(line);
        let spans = b.highlighter.get(line);
        let dl = Rc::new(DisplayLine::build(&text, tab, show_ws));
        let len_chars = dl.char_count();
        let text_system = window.text_system().clone();
        let boundaries = match self.display.wrap_width {
            Some(w) => {
                let font_id = text_system.resolve_font(&style.font);
                let size = style.font_size;
                let mut measure = |c: char| {
                    text_system.advance(font_id, size, c).map(|s| f32::from(s.width)).unwrap_or(0.0)
                };
                display::wrap_boundaries(&dl.text, w, &mut self.display.widths, &mut measure)
            }
            None => Vec::new(),
        };
        let mut segs = Vec::with_capacity(boundaries.len() + 1);
        let mut starts = vec![0usize];
        starts.extend(boundaries.iter().copied());
        for (i, &ds) in starts.iter().enumerate() {
            let de = starts.get(i + 1).copied().unwrap_or(dl.text.len());
            let runs = Self::build_runs(&dl, spans.as_deref(), ds..de, &style, &theme, if self.mode == EditorMode::Full { theme.fg } else { theme.panel_fg });
            let seg_text: SharedString = dl.text[ds..de].to_string().into();
            let shaped = text_system.shape_line(seg_text, style.font_size, &runs, None);
            let cs = dl.disp_to_char(ds);
            let ce = if i + 1 < starts.len() { dl.disp_to_char(de) } else { len_chars };
            segs.push(RowSeg { disp: ds..de, chars: cs..ce, shaped });
        }
        LineShape { line, start_char, len_chars, dl, segs }
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut key_context = KeyContext::new_with_defaults();
        key_context.add("Editor");
        key_context.set(
            "mode",
            match self.mode {
                EditorMode::Full => "full",
                EditorMode::SingleLine => "single_line",
                EditorMode::MultiLine => "multi_line",
            },
        );
        if self.completion.is_some() {
            key_context.add("completing");
        }
        if self.rect.is_some() {
            key_context.add("column_selecting");
        }
        let theme = crate::theme::theme(cx);
        let popup = self.render_completion(window, cx, &theme);
        let root = div()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .size_full()
            .relative()
            .child(EditorElement::new(cx.entity()))
            .children(popup);
        let root = Self::register_basic_actions(root, cx);
        if self.mode == EditorMode::Full {
            Self::register_doc_actions(root, cx)
        } else {
            root
        }
    }
}
