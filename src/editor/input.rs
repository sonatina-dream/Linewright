//! Platform text input (typing, IME composition, dictation) for the editor.

use std::ops::Range;

use gpui::{point, px, Bounds, Context, EntityInputHandler, Pixels, Point, UTF16Selection, Window};

use super::Editor;
use crate::buffer::{EditKind, Selection};

impl Editor {
    fn utf16_range_to_chars(&self, r: &Range<usize>, cx: &gpui::App) -> Range<usize> {
        let b = self.buf(cx);
        b.utf16_to_char(r.start)..b.utf16_to_char(r.end)
    }

    fn chars_range_to_utf16(&self, r: &Range<usize>, cx: &gpui::App) -> Range<usize> {
        let b = self.buf(cx);
        b.char_to_utf16(r.start)..b.char_to_utf16(r.end)
    }

    /// Screen bounds of a char position (for IME candidate windows).
    fn char_bounds(&self, pos: usize) -> Option<Bounds<Pixels>> {
        let layout = self.layout.as_ref()?;
        for row in &layout.rows {
            let start = row.shape.start_char;
            if pos < start || pos > start + row.shape.len_chars {
                continue;
            }
            let col = pos - start;
            if row.shape.seg_for_char(col) != row.sub {
                continue;
            }
            let (_, x) = row.shape.x_for_char(col);
            let x = self.text_left + x - self.scroll_x;
            return Some(Bounds::new(point(px(x), px(row.y)), gpui::size(px(2.0), px(layout.line_height))));
        }
        None
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let r = self.utf16_range_to_chars(&range_utf16, cx);
        actual_range.replace(self.chars_range_to_utf16(&r, cx));
        Some(self.buf(cx).slice_string(r))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let s = self.primary_selection();
        Some(UTF16Selection { range: self.chars_range_to_utf16(&s.range(), cx), reversed: s.reversed() })
    }

    fn marked_text_range(&self, _window: &mut Window, cx: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range.as_ref().map(|r| self.chars_range_to_utf16(r, cx))
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.marked_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let explicit = range_utf16.as_ref().map(|r| self.utf16_range_to_chars(r, cx)).or(self.marked_range.clone());
        self.marked_range = None;
        match explicit {
            Some(range) if self.selections.len() <= 1 || range != self.primary_selection().range() => {
                // Replace a specific range (IME commit / press-and-hold accent menu).
                let n = text.chars().count();
                if self.mode == super::EditorMode::Full && self.buf(cx).is_read_only() {
                    return;
                }
                self.record_step(super::MacroStep::Insert(text.to_string()), cx);
                self.transact(vec![(range.clone(), text.to_string())], EditKind::Typing, cx);
                self.rect = None;
                self.set_selections(vec![Selection::caret(range.start + n)], cx);
                self.finish_transaction(cx);
            }
            _ => self.insert_text(text, window, cx),
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == super::EditorMode::Full && self.buf(cx).is_read_only() {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|r| self.utf16_range_to_chars(r, cx))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.primary_selection().range());
        let n = new_text.chars().count();
        self.transact(vec![(range.clone(), new_text.to_string())], EditKind::Typing, cx);
        self.marked_range = if n > 0 { Some(range.start..range.start + n) } else { None };
        let sel = match new_selected_range_utf16 {
            Some(sr) => {
                // Relative to the inserted text, in UTF-16 units.
                let to_chars = |u: usize| -> usize {
                    let mut acc = 0;
                    let mut chars = 0;
                    for c in new_text.chars() {
                        if acc >= u {
                            break;
                        }
                        acc += c.len_utf16();
                        chars += 1;
                    }
                    chars
                };
                Selection::new(range.start + to_chars(sr.start), range.start + to_chars(sr.end))
            }
            None => Selection::caret(range.start + n),
        };
        self.rect = None;
        self.set_selections(vec![sel], cx);
        self.finish_transaction(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let r = self.utf16_range_to_chars(&range_utf16, cx);
        let a = self.char_bounds(r.start)?;
        let b = self.char_bounds(r.end).unwrap_or(a);
        Some(Bounds::from_corners(a.origin, point(b.origin.x.max(a.origin.x + px(1.0)), a.origin.y + a.size.height)))
    }

    fn character_index_for_point(&mut self, p: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) -> Option<usize> {
        let (offset, _, _) = self.hit_test(p, window, cx);
        Some(self.buf(cx).char_to_utf16(offset))
    }
}
