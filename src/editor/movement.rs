//! Caret movement and selection commands.

use gpui::{Context, Window};
use unicode_segmentation::UnicodeSegmentation;

use super::{Autoscroll, Editor, EditorEvent, EditorMode, RectSelection};
use crate::buffer::{Buffer, Selection};
use crate::tools::is_word_char;

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Space,
    Word,
    Punct,
}

fn class_of(c: char) -> CharClass {
    if c.is_whitespace() {
        CharClass::Space
    } else if is_word_char(c) {
        CharClass::Word
    } else {
        CharClass::Punct
    }
}

pub fn prev_grapheme(b: &Buffer, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let line = b.char_to_line(pos);
    let start = b.line_to_char(line);
    let col = pos - start;
    if col == 0 {
        return b.line_end(line - 1);
    }
    let text = b.line_text(line);
    let byte_col: usize = text.chars().take(col).map(|c| c.len_utf8()).sum();
    let mut prev = 0;
    for (i, _) in text.grapheme_indices(true) {
        if i >= byte_col {
            break;
        }
        prev = i;
    }
    start + text[..prev].chars().count()
}

pub fn next_grapheme(b: &Buffer, pos: usize) -> usize {
    let line = b.char_to_line(pos);
    let end = b.line_end(line);
    if pos >= end {
        return if line + 1 < b.len_lines() { b.line_to_char(line + 1) } else { pos.min(b.len_chars()) };
    }
    let start = b.line_to_char(line);
    let col = pos - start;
    let text = b.line_text(line);
    let byte_col: usize = text.chars().take(col).map(|c| c.len_utf8()).sum();
    for (i, g) in text.grapheme_indices(true) {
        if i >= byte_col {
            return start + text[..i + g.len()].chars().count();
        }
    }
    end
}

pub fn word_right(b: &Buffer, pos: usize) -> usize {
    let line = b.char_to_line(pos);
    let end = b.line_end(line);
    if pos >= end {
        return next_grapheme(b, pos);
    }
    let mut p = pos;
    while p < end && b.char_at(p).map(|c| c == ' ' || c == '\t').unwrap_or(false) {
        p += 1;
    }
    if p >= end {
        return p;
    }
    let cls = class_of(b.char_at(p).unwrap_or(' '));
    while p < end && b.char_at(p).map(class_of) == Some(cls) {
        p += 1;
    }
    p
}

pub fn word_left(b: &Buffer, pos: usize) -> usize {
    let line = b.char_to_line(pos);
    let start = b.line_to_char(line);
    if pos <= start {
        return prev_grapheme(b, pos);
    }
    let mut p = pos;
    while p > start && b.char_at(p - 1).map(|c| c == ' ' || c == '\t').unwrap_or(false) {
        p -= 1;
    }
    if p <= start {
        return p;
    }
    let cls = class_of(b.char_at(p - 1).unwrap_or(' '));
    while p > start && b.char_at(p - 1).map(class_of) == Some(cls) {
        p -= 1;
    }
    p
}

/// Range of the word around `pos` (word chars, or a run of punctuation / spaces).
pub fn word_range_at(b: &Buffer, pos: usize) -> std::ops::Range<usize> {
    let line = b.char_to_line(pos);
    let start = b.line_to_char(line);
    let end = b.line_end(line);
    if start == end {
        return pos..pos;
    }
    let probe = if pos < end && b.char_at(pos).map(is_word_char).unwrap_or(false) {
        pos
    } else if pos > start && b.char_at(pos - 1).map(is_word_char).unwrap_or(false) {
        pos - 1
    } else if pos < end {
        pos
    } else {
        pos - 1
    };
    let cls = class_of(b.char_at(probe).unwrap_or(' '));
    let mut s = probe;
    while s > start && b.char_at(s - 1).map(class_of) == Some(cls) {
        s -= 1;
    }
    let mut e = probe;
    while e < end && b.char_at(e).map(class_of) == Some(cls) {
        e += 1;
    }
    s..e
}

/// The identifier around `pos`, if any.
pub fn word_at(b: &Buffer, pos: usize) -> Option<std::ops::Range<usize>> {
    let r = word_range_at(b, pos);
    if r.is_empty() {
        return None;
    }
    if b.char_at(r.start).map(is_word_char).unwrap_or(false) {
        Some(r)
    } else {
        None
    }
}

const BRACES: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

/// If there is a brace at or just before `pos`, return (brace_pos, matching_pos or None).
pub fn brace_match(b: &Buffer, pos: usize) -> Option<(usize, Option<usize>)> {
    let check = |p: usize| -> Option<(usize, char)> {
        let c = b.char_at(p)?;
        if BRACES.iter().any(|(o, cl)| *o == c || *cl == c) {
            Some((p, c))
        } else {
            None
        }
    };
    let (bp, c) = (if pos > 0 { check(pos - 1) } else { None }).or_else(|| check(pos))?;
    let (open, close, forward) = BRACES
        .iter()
        .find_map(|(o, cl)| if *o == c { Some((*o, *cl, true)) } else if *cl == c { Some((*o, *cl, false)) } else { None })?;
    let rope = b.rope();
    let limit = 2_000_000usize;
    let mut depth = 0i32;
    if forward {
        let mut it = rope.chars_at(bp);
        let mut p = bp;
        let mut n = 0;
        while let Some(ch) = it.next() {
            if ch == open {
                depth += 1;
            } else if ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some((bp, Some(p)));
                }
            }
            p += 1;
            n += 1;
            if n > limit {
                break;
            }
        }
    } else {
        let mut it = rope.chars_at(bp + 1);
        let mut p = bp + 1;
        let mut n = 0;
        while let Some(ch) = it.prev() {
            p -= 1;
            if ch == close {
                depth += 1;
            } else if ch == open {
                depth -= 1;
                if depth == 0 {
                    return Some((bp, Some(p)));
                }
            }
            n += 1;
            if n > limit {
                break;
            }
        }
    }
    Some((bp, None))
}

impl Editor {
    pub fn ensure_display(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let style = self.style(cx);
        let text_system = window.text_system().clone();
        let font_id = text_system.resolve_font(&style.font);
        let size = style.font_size;
        let key = (style.font.family.clone(), f32::from(size));
        if key != self.font_key {
            self.font_key = key;
            self.display.widths = Default::default();
            self.display.invalidate();
        }
        let mut measure = |c: char| text_system.advance(font_id, size, c).map(|s| f32::from(s.width)).unwrap_or(0.0);
        self.display.tab_size = self.tab_size(cx);
        let b = self.buffer.read(cx);
        self.display.ensure(b, &self.folds, &mut measure);
    }

    /// Move every selection with `f(head) -> new head`. When `select` is false the selection collapses.
    fn move_each(
        &mut self,
        select: bool,
        cx: &mut Context<Self>,
        mut f: impl FnMut(&Buffer, Selection) -> usize,
    ) {
        self.rect = None;
        let b = self.buffer.read(cx);
        let sels: Vec<Selection> = self
            .selections
            .iter()
            .map(|s| {
                let head = f(b, *s);
                if select {
                    Selection { anchor: s.anchor, head, goal: None }
                } else {
                    Selection::caret(head)
                }
            })
            .collect();
        let primary_head = sels.get(self.primary).map(|s| s.head);
        self.set_selections_with_primary(sels, primary_head, cx);
    }

    pub fn move_left(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(false, cx, |b, s| if s.is_empty() { prev_grapheme(b, s.head) } else { s.start() });
    }
    pub fn move_right(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(false, cx, |b, s| if s.is_empty() { next_grapheme(b, s.head) } else { s.end() });
    }
    pub fn select_left(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(true, cx, |b, s| prev_grapheme(b, s.head));
    }
    pub fn select_right(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(true, cx, |b, s| next_grapheme(b, s.head));
    }
    pub fn move_word_left(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(false, cx, |b, s| word_left(b, s.head));
    }
    pub fn move_word_right(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(false, cx, |b, s| word_right(b, s.head));
    }
    pub fn select_word_left(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(true, cx, |b, s| word_left(b, s.head));
    }
    pub fn select_word_right(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(true, cx, |b, s| word_right(b, s.head));
    }

    fn smart_home(b: &Buffer, pos: usize) -> usize {
        let line = b.char_to_line(pos);
        let start = b.line_to_char(line);
        let text = b.line_text(line);
        let indent = text.chars().take_while(|c| *c == ' ' || *c == '\t').count();
        if pos == start + indent {
            start
        } else {
            start + indent
        }
    }

    pub fn move_to_line_start(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.line_start_impl(false, w, cx);
    }
    pub fn select_to_line_start(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.line_start_impl(true, w, cx);
    }

    fn line_start_impl(&mut self, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        // Wrapped rows: first go to the start of the visual row.
        let wrap = self.display.wrap_width.is_some();
        let mut targets = Vec::with_capacity(self.selections.len());
        for s in self.selections.clone() {
            let b = self.buffer.read(cx);
            let line = b.char_to_line(s.head);
            let mut target = Self::smart_home(b, s.head);
            if wrap {
                let shape = self.shape_line(line, window, cx);
                let col = s.head - shape.start_char;
                let si = shape.seg_for_char(col);
                if si > 0 {
                    let seg_start = shape.start_char + shape.segs[si].chars.start;
                    if s.head != seg_start {
                        target = seg_start;
                    }
                }
            }
            targets.push(target);
        }
        let mut i = 0;
        self.move_each(select, cx, |_, _| {
            let t = targets[i];
            i += 1;
            t
        });
    }

    pub fn move_to_line_end(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(false, cx, |b, s| b.line_end(b.char_to_line(s.head)));
    }
    pub fn select_to_line_end(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(true, cx, |b, s| b.line_end(b.char_to_line(s.head)));
    }
    pub fn move_to_doc_start(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(false, cx, |_, _| 0);
        self.collapse_to_primary(cx);
    }
    pub fn move_to_doc_end(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(false, cx, |b, _| b.len_chars());
        self.collapse_to_primary(cx);
    }
    pub fn select_to_doc_start(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(true, cx, |_, _| 0);
    }
    pub fn select_to_doc_end(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_each(true, cx, |b, _| b.len_chars());
    }

    fn collapse_to_primary(&mut self, cx: &mut Context<Self>) {
        if self.selections.len() > 1 {
            let p = self.primary_selection();
            self.set_selections(vec![p], cx);
        }
    }

    /// Compute (new position, goal x) for moving `rows` display rows from `pos`.
    pub(crate) fn vertical_target(
        &mut self,
        pos: usize,
        goal: Option<f32>,
        rows: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (usize, f32) {
        self.ensure_display(window, cx);
        let line = self.buf(cx).char_to_line(pos);
        let shape = self.shape_line(line, window, cx);
        let col = pos - shape.start_char;
        let (seg, x) = shape.x_for_char(col);
        let goal_x = goal.unwrap_or(x);
        let row = self.display.line_to_row(line) + seg;
        let row_count = self.display.row_count(self.buf(cx));
        let target = row as isize + rows;
        if target < 0 {
            return (0, goal_x);
        }
        if target as usize >= row_count {
            return (self.buf(cx).len_chars(), goal_x);
        }
        let (tline, tsub) = self.display.row_to_line(target as usize);
        let tshape = self.shape_line(tline, window, cx);
        let tcol = tshape.char_for_x(tsub, goal_x);
        (tshape.start_char + tcol, goal_x)
    }

    fn vertical_move(&mut self, rows: isize, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.rect = None;
        if self.mode == EditorMode::SingleLine {
            if select {
                let len = self.buf(cx).len_chars();
                self.move_each(true, cx, |_, _| if rows < 0 { 0 } else { len });
            } else {
                let len = self.buf(cx).len_chars();
                self.move_each(false, cx, |_, _| if rows < 0 { 0 } else { len });
            }
            return;
        }
        let mut out = Vec::with_capacity(self.selections.len());
        for s in self.selections.clone() {
            let from = if select || s.is_empty() { s.head } else if rows < 0 { s.start() } else { s.end() };
            let (pos, goal) = self.vertical_target(from, s.goal, rows, window, cx);
            if select {
                out.push(Selection { anchor: s.anchor, head: pos, goal: Some(goal) });
            } else {
                out.push(Selection { anchor: pos, head: pos, goal: Some(goal) });
            }
        }
        let ph = out.get(self.primary).map(|s| s.head);
        self.set_selections_with_primary(out, ph, cx);
    }

    pub fn move_up(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.vertical_move(-1, false, w, cx);
    }
    pub fn move_down(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.vertical_move(1, false, w, cx);
    }
    pub fn select_up(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.vertical_move(-1, true, w, cx);
    }
    pub fn select_down(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.vertical_move(1, true, w, cx);
    }

    pub(crate) fn page_rows(&self) -> isize {
        self.layout
            .as_ref()
            .map(|l| ((f32::from(l.text_bounds.size.height) / l.line_height).floor() as isize - 1).max(1))
            .unwrap_or(20)
    }

    fn page_move(&mut self, dir: isize, select: bool, w: &mut Window, cx: &mut Context<Self>) {
        let rows = self.page_rows();
        let lh = self.layout.as_ref().map(|l| l.line_height).unwrap_or(18.0);
        self.scroll_y = (self.scroll_y + dir as f32 * rows as f32 * lh).max(0.0);
        self.vertical_move(dir * rows, select, w, cx);
        cx.emit(EditorEvent::Scrolled);
    }

    pub fn page_up(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.page_move(-1, false, w, cx);
    }
    pub fn page_down(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.page_move(1, false, w, cx);
    }
    pub fn select_page_up(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.page_move(-1, true, w, cx);
    }
    pub fn select_page_down(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.page_move(1, true, w, cx);
    }

    pub fn select_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.rect = None;
        let len = self.buf(cx).len_chars();
        self.set_selections(vec![Selection::new(0, len)], cx);
        self.autoscroll = None;
    }

    pub fn cancel(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.completion.take().is_some() {
            cx.notify();
            return;
        }
        match self.mode {
            EditorMode::SingleLine | EditorMode::MultiLine => cx.emit(EditorEvent::Cancel),
            EditorMode::Full => {
                if self.selections.len() > 1 || self.rect.is_some() {
                    self.rect = None;
                    let p = self.primary_selection();
                    self.set_selections(vec![Selection::caret(p.head)], cx);
                } else if !self.incremental_highlight.is_empty() {
                    self.incremental_highlight.clear();
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }
        }
    }

    // ------------------------------------------------------------------ column / multi-cursor

    pub(crate) fn caret_line_x(&mut self, pos: usize, window: &mut Window, cx: &mut Context<Self>) -> (usize, f32) {
        let line = self.buf(cx).char_to_line(pos);
        let shape = self.shape_line(line, window, cx);
        let (_, x) = shape.x_for_char(pos - shape.start_char);
        (line, x)
    }

    pub(crate) fn apply_rect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.rect else { return };
        self.ensure_display(window, cx);
        let (l0, l1) = (r.anchor_line.min(r.head_line), r.anchor_line.max(r.head_line));
        let mut sels = Vec::new();
        let mut primary_head = None;
        for line in l0..=l1 {
            if !self.display.is_line_visible(line) {
                continue;
            }
            let shape = self.shape_line(line, window, cx);
            let a = shape.char_for_x(0, r.anchor_x);
            let h = shape.char_for_x(0, r.head_x);
            let sel = Selection::new(shape.start_char + a, shape.start_char + h);
            if line == r.head_line {
                primary_head = Some(sel.head);
            }
            sels.push(sel);
        }
        let saved = self.rect;
        self.set_selections_with_primary(sels, primary_head, cx);
        self.rect = saved;
    }

    fn column_select(&mut self, dl: isize, dx: isize, window: &mut Window, cx: &mut Context<Self>) {
        let space = self.layout.as_ref().map(|l| l.space_width).unwrap_or(8.0);
        let mut r = match self.rect {
            Some(r) => r,
            None => {
                let head = self.primary_selection().head;
                let (line, x) = self.caret_line_x(head, window, cx);
                let anchor = self.primary_selection().anchor;
                let (aline, ax) = self.caret_line_x(anchor, window, cx);
                RectSelection { anchor_line: aline, anchor_x: ax, head_line: line, head_x: x }
            }
        };
        let max_line = self.buf(cx).len_lines().saturating_sub(1);
        r.head_line = (r.head_line as isize + dl).clamp(0, max_line as isize) as usize;
        r.head_x = (r.head_x + dx as f32 * space).max(0.0);
        // Snap to char grid.
        r.head_x = (r.head_x / space).round() * space;
        self.rect = Some(r);
        self.apply_rect(window, cx);
    }

    pub fn column_select_up(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.column_select(-1, 0, w, cx);
    }
    pub fn column_select_down(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.column_select(1, 0, w, cx);
    }
    pub fn column_select_left(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.column_select(0, -1, w, cx);
    }
    pub fn column_select_right(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.column_select(0, 1, w, cx);
    }

    fn add_cursor_vertical(&mut self, dir: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.rect = None;
        let edge = if dir < 0 {
            *self.selections.iter().min_by_key(|s| s.head).unwrap()
        } else {
            *self.selections.iter().max_by_key(|s| s.head).unwrap()
        };
        let (pos, goal) = self.vertical_target(edge.head, edge.goal, dir, window, cx);
        let line_now = self.buf(cx).char_to_line(edge.head);
        let line_new = self.buf(cx).char_to_line(pos);
        if line_now == line_new {
            return;
        }
        let mut sels = self.selections.clone();
        sels.push(Selection { anchor: pos, head: pos, goal: Some(goal) });
        self.set_selections_with_primary(sels, Some(pos), cx);
    }

    pub fn add_cursor_above(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.add_cursor_vertical(-1, w, cx);
    }
    pub fn add_cursor_below(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.add_cursor_vertical(1, w, cx);
    }

    pub fn toggle_selection_anchor(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.selection_anchor_impl(false, w, cx);
    }
    pub fn toggle_column_selection_anchor(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.selection_anchor_impl(true, w, cx);
    }

    fn selection_anchor_impl(&mut self, column: bool, window: &mut Window, cx: &mut Context<Self>) {
        let caret = self.caret();
        match self.selection_anchor.take() {
            None => {
                self.selection_anchor = Some((caret, column));
                cx.emit(EditorEvent::Notify(if column {
                    "Begin column select: move the caret and choose End Select".into()
                } else {
                    "Begin select: move the caret and choose End Select".into()
                }));
            }
            Some((anchor, col)) => {
                if col || column {
                    let (al, ax) = self.caret_line_x(anchor, window, cx);
                    let (hl, hx) = self.caret_line_x(caret, window, cx);
                    self.rect = Some(RectSelection { anchor_line: al, anchor_x: ax, head_line: hl, head_x: hx });
                    self.apply_rect(window, cx);
                } else {
                    self.set_selections(vec![Selection::new(anchor, caret)], cx);
                }
            }
        }
        cx.notify();
    }

    pub fn select_line(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let sels: Vec<Selection> = self
            .selections
            .iter()
            .map(|s| {
                let l0 = b.char_to_line(s.start());
                let l1 = b.char_to_line(s.end());
                let start = b.line_to_char(l0);
                let end = if l1 + 1 < b.len_lines() { b.line_to_char(l1 + 1) } else { b.len_chars() };
                Selection::new(start, end)
            })
            .collect();
        self.set_selections(sels, cx);
    }

    pub fn select_word(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let sels: Vec<Selection> = self
            .selections
            .iter()
            .map(|s| {
                if s.is_empty() {
                    let r = word_range_at(b, s.head);
                    Selection::new(r.start, r.end)
                } else {
                    *s
                }
            })
            .collect();
        self.set_selections(sels, cx);
    }

    /// Add the next occurrence of the primary selection as a new selection (multi-select next).
    pub fn select_next_occurrence(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let p = self.primary_selection();
        if p.is_empty() {
            self.select_word(w, cx);
            return;
        }
        let needle = self.selected_text(cx);
        if needle.is_empty() {
            return;
        }
        let text = self.buffer.update(cx, |b, _| b.text_arc());
        let b = self.buf(cx);
        let from_byte = b.char_to_byte(self.selections.iter().map(|s| s.end()).max().unwrap_or(0));
        let found = text[from_byte..]
            .find(&needle)
            .map(|i| from_byte + i)
            .or_else(|| text.find(&needle));
        if let Some(start_b) = found {
            let start = b.byte_to_char(start_b);
            let end = b.byte_to_char(start_b + needle.len());
            if self.selections.iter().any(|s| s.start() == start) {
                return;
            }
            let mut sels = self.selections.clone();
            sels.push(Selection::new(start, end));
            self.set_selections_with_primary(sels, Some(end), cx);
        }
    }

    pub fn go_to_matching_brace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let caret = self.caret();
        if let Some((bp, Some(m))) = brace_match(self.buf(cx), caret) {
            let target = if m > bp { m + 1 } else { m };
            let target = if caret == bp { if m > bp { m } else { m } } else { target };
            self.set_selections(vec![Selection::caret(target)], cx);
        }
    }

    pub fn select_to_matching_brace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let caret = self.caret();
        if let Some((bp, Some(m))) = brace_match(self.buf(cx), caret) {
            let (s, e) = if m > bp { (bp, m + 1) } else { (m, bp + 1) };
            self.set_selections(vec![Selection::new(s, e)], cx);
        }
    }

    pub fn scroll_line_up(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let lh = self.layout.as_ref().map(|l| l.line_height).unwrap_or(18.0);
        self.scroll_y = (self.scroll_y - lh).max(0.0);
        cx.emit(EditorEvent::Scrolled);
        cx.notify();
    }

    pub fn scroll_line_down(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let lh = self.layout.as_ref().map(|l| l.line_height).unwrap_or(18.0);
        self.scroll_y += lh;
        cx.emit(EditorEvent::Scrolled);
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn scroll_to_line_top(&mut self, line: usize, cx: &mut Context<Self>) {
        let lh = self.layout.as_ref().map(|l| l.line_height).unwrap_or(18.0);
        self.scroll_y = self.display.line_to_row(line) as f32 * lh;
        self.autoscroll = None;
        cx.notify();
    }

    pub fn set_scroll(&mut self, x: Option<f32>, y: Option<f32>, cx: &mut Context<Self>) {
        if let Some(x) = x {
            self.scroll_x = x.max(0.0);
        }
        if let Some(y) = y {
            self.scroll_y = y.max(0.0);
        }
        self.autoscroll = None;
        cx.notify();
    }

    #[allow(dead_code)]
    pub fn center_on_caret(&mut self, cx: &mut Context<Self>) {
        self.request_autoscroll(Autoscroll::Center, cx);
    }
}

/// If the caret is inside an HTML/XML tag, return the tag-name ranges of that tag and its partner.
pub fn tag_match(b: &Buffer, pos: usize) -> Option<(std::ops::Range<usize>, Option<std::ops::Range<usize>>)> {
    const WINDOW: usize = 200_000;
    let len = b.len_chars();
    let from = pos.saturating_sub(WINDOW);
    let to = (pos + WINDOW).min(len);
    let s = b.slice_string(from..to);
    let caret_b: usize = s.char_indices().nth(pos - from).map(|(i, _)| i).unwrap_or(s.len());
    let lt = s[..caret_b].rfind('<')?;
    if let Some(gt) = s[..caret_b].rfind('>') {
        if gt > lt {
            return None;
        }
    }
    let tag_end = lt + s[lt..].find('>')?;
    let tag = &s[lt..=tag_end];
    if tag.starts_with("<!") || tag.starts_with("<?") || tag.ends_with("/>") {
        return None;
    }
    let closing = tag.starts_with("</");
    let name_start = lt + if closing { 2 } else { 1 };
    let name: String = s[name_start..].chars().take_while(|c| c.is_alphanumeric() || matches!(c, '-' | ':' | '_' | '.')).collect();
    if name.is_empty() {
        return None;
    }
    let to_chars = |byte: usize| from + s[..byte].chars().count();
    let own = to_chars(name_start)..to_chars(name_start + name.len());
    let re = regex::Regex::new(&format!(r"<(/?){}(?:[\s/>])", regex::escape(&name))).ok()?;
    let is_self_closing = |start: usize| -> bool {
        s[start..].find('>').map(|e| s[start..start + e + 1].ends_with("/>")).unwrap_or(false)
    };
    if !closing {
        let mut depth = 1i32;
        for m in re.find_iter(&s[tag_end + 1..]) {
            let abs = tag_end + 1 + m.start();
            let is_close = s[abs..].starts_with("</");
            if is_close {
                depth -= 1;
                if depth == 0 {
                    let ns = abs + 2;
                    return Some((own, Some(to_chars(ns)..to_chars(ns + name.len()))));
                }
            } else if !is_self_closing(abs) {
                depth += 1;
            }
        }
        Some((own, None))
    } else {
        let tags: Vec<usize> = re.find_iter(&s[..lt]).map(|m| m.start()).collect();
        let mut depth = 1i32;
        for &abs in tags.iter().rev() {
            let is_close = s[abs..].starts_with("</");
            if is_close {
                depth += 1;
            } else if !is_self_closing(abs) {
                depth -= 1;
                if depth == 0 {
                    let ns = abs + 1;
                    return Some((own, Some(to_chars(ns)..to_chars(ns + name.len()))));
                }
            }
        }
        Some((own, None))
    }
}

#[cfg(test)]
mod tag_tests {
    #[test]
    fn matches_nested_tags() {
        let mut b = crate::buffer::Buffer::new(1, "t".into(), crate::encoding::EolMode::Lf, Default::default());
        b.set_text_initial("<div><div a='1'>x</div><br/></div>");
        // caret inside the first opening tag
        let (own, other) = super::tag_match(&b, 2).unwrap();
        assert_eq!(own, 1..4);
        assert_eq!(other, Some(30..33));
        // caret inside the inner closing tag
        let (own, other) = super::tag_match(&b, 20).unwrap();
        assert_eq!(own, 19..22);
        assert_eq!(other, Some(6..9));
    }
}
