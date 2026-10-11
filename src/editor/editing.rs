//! Text-modifying editor commands.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    deferred, div, anchored, prelude::*, px, AnyElement, ClipboardItem, Context, Global, MouseButton, Window,
};

use super::movement::{next_grapheme, prev_grapheme, word_at, word_left, word_right};
use super::{Completion, Editor, EditorEvent, EditorMode, MacroStep};
use crate::buffer::{map_pos, Buffer, EditKind, Mark, Selection};
use crate::settings::Settings;
use crate::theme::AppTheme;
use crate::tools::{self, CaseMode, LineOp};

const CLIP_LINE: &str = "linewright:line";
const CLIP_MULTI: &str = "linewright:multi";

/// Recently copied texts (Edit > Clipboard History panel).
#[derive(Default)]
pub struct ClipboardHistory {
    pub items: Vec<String>,
}

impl Global for ClipboardHistory {}

pub fn push_clipboard_history(cx: &mut gpui::App, text: &str) {
    if text.is_empty() {
        return;
    }
    if cx.try_global::<ClipboardHistory>().is_none() {
        cx.set_global(ClipboardHistory::default());
    }
    let h = cx.global_mut::<ClipboardHistory>();
    h.items.retain(|t| t != text);
    h.items.insert(0, text.to_string());
    h.items.truncate(40);
}

struct Completer {
    words: Rc<Vec<String>>,
}

impl Editor {
    fn read_only_guard(&self, cx: &mut Context<Self>) -> bool {
        if self.mode == EditorMode::Full && self.buf(cx).is_read_only() {
            cx.emit(EditorEvent::Notify("This document is read-only.".into()));
            return true;
        }
        false
    }

    /// Apply edits (current coordinates) and put a caret / selection after each, as described
    /// by `select_inserted`. Overlapping edits are merged.
    pub(crate) fn apply_edits_with_carets(
        &mut self,
        mut edits: Vec<(Range<usize>, String)>,
        select_inserted: bool,
        kind: EditKind,
        cx: &mut Context<Self>,
    ) {
        if self.read_only_guard(cx) || edits.is_empty() {
            return;
        }
        edits.sort_by_key(|(r, _)| (r.start, r.end));
        let mut merged: Vec<(Range<usize>, String)> = Vec::with_capacity(edits.len());
        for (r, t) in edits {
            if let Some((lr, lt)) = merged.last_mut() {
                if r.start < lr.end || (r.start == lr.start && r.start == r.end && lr.start == lr.end) {
                    if r.start == lr.start && r.start == r.end && lr.start == lr.end && t == *lt {
                        continue;
                    }
                    lr.end = lr.end.max(r.end);
                    lt.push_str(&t);
                    continue;
                }
            }
            merged.push((r, t));
        }
        let mut delta: isize = 0;
        let mut sels = Vec::with_capacity(merged.len());
        for (r, t) in &merged {
            let start = (r.start as isize + delta) as usize;
            let n = t.chars().count();
            sels.push(if select_inserted { Selection::new(start, start + n) } else { Selection::caret(start + n) });
            delta += n as isize - (r.end - r.start) as isize;
        }
        self.transact(merged, kind, cx);
        self.rect = None;
        self.set_selections(sels, cx);
        self.finish_transaction(cx);
    }

    /// Apply edits and map the current selections through them.
    pub(crate) fn apply_edits_mapping(&mut self, edits: Vec<(Range<usize>, String)>, cx: &mut Context<Self>) {
        if self.read_only_guard(cx) || edits.is_empty() {
            return;
        }
        let old = self.selections.clone();
        let infos = self.transact(edits, EditKind::Other, cx);
        let sels: Vec<Selection> = old
            .iter()
            .map(|s| {
                if s.is_empty() {
                    Selection::caret(map_pos(s.head, &infos, true))
                } else if s.reversed() {
                    Selection::new(map_pos(s.anchor, &infos, true), map_pos(s.head, &infos, false))
                } else {
                    Selection::new(map_pos(s.anchor, &infos, false), map_pos(s.head, &infos, true))
                }
            })
            .collect();
        self.rect = None;
        self.set_selections(sels, cx);
        self.finish_transaction(cx);
    }

    // ------------------------------------------------------------------ typing

    pub fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        self.record_step(MacroStep::Insert(text.to_string()), cx);
        let text = if self.mode == EditorMode::SingleLine { text.replace(['\r', '\n'], " ") } else { text.to_string() };
        let settings = cx.global::<Settings>().clone();
        let single_char = {
            let mut it = text.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Some(c),
                _ => None,
            }
        };
        let full = self.mode == EditorMode::Full;
        let b = self.buffer.read(cx);
        let mut edits: Vec<(Range<usize>, String)> = Vec::new();
        let mut caret_after: Vec<Option<usize>> = Vec::new(); // explicit caret offset within inserted text
        let mut skip_only: Vec<bool> = Vec::new();
        for s in &self.selections {
            let mut range = s.range();
            let mut ins = text.clone();
            let mut caret_off = None;
            let mut skip = false;
            if let Some(c) = single_char {
                let next = b.char_at(s.head).filter(|c| *c != '\n' && *c != '\r');
                let closers = [(')', '('), (']', '['), ('}', '{'), ('"', '"'), ('\'', '\''), ('`', '`')];
                let auto_br = full && settings.auto_close_brackets;
                let auto_q = full && settings.auto_close_quotes;
                let is_quote = c == '"' || c == '\'' || c == '`';
                let enabled = if is_quote { auto_q } else { auto_br };
                if enabled && s.is_empty() && next == Some(c) && closers.iter().any(|(cl, _)| *cl == c) {
                    // Type over the auto-inserted closer.
                    skip = true;
                } else if enabled {
                    let pair = match c {
                        '(' => Some(')'),
                        '[' => Some(']'),
                        '{' => Some('}'),
                        '"' => Some('"'),
                        '\'' => Some('\''),
                        '`' => Some('`'),
                        _ => None,
                    };
                    if let Some(close) = pair {
                        if !s.is_empty() {
                            let inner = b.slice_string(s.range());
                            ins = format!("{c}{inner}{close}");
                            caret_off = Some(usize::MAX); // select inner
                        } else {
                            let prev = if s.head > 0 { b.char_at(s.head - 1) } else { None };
                            let next_ok = next.map(|n| n.is_whitespace() || ")]};,".contains(n)).unwrap_or(true);
                            let prev_ok = !is_quote || prev.map(|p| !crate::tools::is_word_char(p)).unwrap_or(true);
                            if next_ok && prev_ok {
                                ins = format!("{c}{close}");
                                caret_off = Some(1);
                            }
                        }
                    }
                }
                if !skip && self.overwrite && s.is_empty() {
                    let line_end = b.line_end(b.char_to_line(s.head));
                    if s.head < line_end {
                        range = s.head..next_grapheme(b, s.head);
                    }
                }
            }
            skip_only.push(skip);
            caret_after.push(caret_off);
            edits.push((range, ins));
        }
        if skip_only.iter().all(|s| *s) {
            let sels: Vec<Selection> = self.selections.iter().map(|s| Selection::caret(s.head + 1)).collect();
            self.set_selections(sels, cx);
            return;
        }
        if self.read_only_guard(cx) {
            return;
        }
        // Compute resulting selections.
        let mut delta: isize = 0;
        let mut sels = Vec::new();
        let mut real_edits = Vec::new();
        let mut order: Vec<usize> = (0..edits.len()).collect();
        order.sort_by_key(|i| edits[*i].0.start);
        for i in order {
            let (r, t) = &edits[i];
            if skip_only[i] {
                let p = (r.start as isize + delta) as usize + 1;
                sels.push(Selection::caret(p));
                continue;
            }
            let start = (r.start as isize + delta) as usize;
            let n = t.chars().count();
            match caret_after[i] {
                Some(usize::MAX) => sels.push(Selection::new(start + 1, start + n - 1)),
                Some(off) => sels.push(Selection::caret(start + off)),
                None => sels.push(Selection::caret(start + n)),
            }
            delta += n as isize - (r.end - r.start) as isize;
            real_edits.push((r.clone(), t.clone()));
        }
        self.transact(real_edits, EditKind::Typing, cx);
        self.rect = None;
        self.set_selections(sels, cx);
        self.finish_transaction(cx);

        if full {
            if single_char == Some('>') && settings.auto_close_html_tags {
                self.auto_close_tag(cx);
            }
            if settings.auto_completion && text.chars().all(crate::tools::is_word_char) {
                if self.completion.is_none() {
                    self.open_completion(false, cx);
                }
            } else if self.completion.is_some() && !text.chars().all(crate::tools::is_word_char) {
                self.completion = None;
            }
        }
        let _ = window;
    }

    fn auto_close_tag(&mut self, cx: &mut Context<Self>) {
        let syn = self.buf(cx).syntax_name.clone();
        if !(syn.contains("HTML") || syn == "XML" || syn.contains("PHP") || syn.contains("JSX") || syn.contains("Vue")) {
            return;
        }
        let caret = self.caret();
        let b = self.buf(cx);
        let start = caret.saturating_sub(400);
        let before = b.slice_string(start..caret);
        let Some(lt) = before.rfind('<') else { return };
        let tag_src = &before[lt..];
        if tag_src.starts_with("</") || tag_src.ends_with("/>") || tag_src.starts_with("<!") || tag_src.starts_with("<?") {
            return;
        }
        let name: String = tag_src[1..].chars().take_while(|c| c.is_alphanumeric() || *c == '-' || *c == ':' || *c == '_').collect();
        if name.is_empty() {
            return;
        }
        let void = ["br", "hr", "img", "input", "meta", "link", "area", "base", "col", "embed", "source", "track", "wbr", "param"];
        if syn.contains("HTML") && void.contains(&name.to_lowercase().as_str()) {
            return;
        }
        let close = format!("</{name}>");
        self.transact(vec![(caret..caret, close)], EditKind::Typing, cx);
        self.set_selections(vec![Selection::caret(caret)], cx);
        self.finish_transaction(cx);
    }

    // ------------------------------------------------------------------ deletion

    fn delete_with(&mut self, cx: &mut Context<Self>, kind: EditKind, f: impl Fn(&Buffer, &Selection) -> Range<usize>) {
        let b = self.buffer.read(cx);
        let edits: Vec<(Range<usize>, String)> = self
            .selections
            .iter()
            .map(|s| if s.is_empty() { (f(b, s), String::new()) } else { (s.range(), String::new()) })
            .filter(|(r, _)| !r.is_empty())
            .collect();
        if edits.is_empty() {
            return;
        }
        self.apply_edits_with_carets(edits, false, kind, cx);
    }

    pub fn backspace(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let auto = self.mode == EditorMode::Full && cx.global::<Settings>().auto_close_brackets;
        self.delete_with(cx, EditKind::Delete, |b, s| {
            let p = prev_grapheme(b, s.head);
            if auto && p + 1 == s.head {
                let pair = (b.char_at(p), b.char_at(s.head));
                if matches!(pair, (Some('('), Some(')')) | (Some('['), Some(']')) | (Some('{'), Some('}')) | (Some('"'), Some('"')) | (Some('\''), Some('\''))) {
                    return p..s.head + 1;
                }
            }
            p..s.head
        });
    }

    pub fn delete(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_with(cx, EditKind::Delete, |b, s| s.head..next_grapheme(b, s.head));
    }
    pub fn delete_word_left(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_with(cx, EditKind::Other, |b, s| word_left(b, s.head)..s.head);
    }
    pub fn delete_word_right(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_with(cx, EditKind::Other, |b, s| s.head..word_right(b, s.head));
    }
    pub fn delete_to_line_start(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_with(cx, EditKind::Other, |b, s| {
            let start = b.line_to_char(b.char_to_line(s.head));
            if start == s.head { prev_grapheme(b, s.head)..s.head } else { start..s.head }
        });
    }
    pub fn delete_to_line_end(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.delete_with(cx, EditKind::Other, |b, s| {
            let end = b.line_end(b.char_to_line(s.head));
            if end == s.head { s.head..next_grapheme(b, s.head) } else { s.head..end }
        });
    }

    // ------------------------------------------------------------------ newline / indent

    fn indent_unit(&self, cx: &gpui::App) -> String {
        let s = cx.global::<Settings>();
        if s.use_spaces {
            " ".repeat(s.tab_size.max(1))
        } else {
            "\t".into()
        }
    }

    pub fn newline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() {
            self.completion_confirm(window, cx);
            return;
        }
        if self.mode == EditorMode::SingleLine {
            cx.emit(EditorEvent::Confirm { shift: window.modifiers().shift });
            return;
        }
        let auto_indent = cx.global::<Settings>().auto_indent;
        let unit = self.indent_unit(cx);
        let b = self.buffer.read(cx);
        let eol = b.eol.as_str().to_string();
        let python = b.syntax_name == "Python" || b.syntax_name == "YAML";
        let mut edits = Vec::new();
        let mut carets = Vec::new();
        for s in &self.selections {
            let line = b.char_to_line(s.start());
            let line_start = b.line_to_char(line);
            let text = b.line_text(line);
            let col = s.start() - line_start;
            let indent: String = if auto_indent { text.chars().take_while(|c| *c == ' ' || *c == '\t').take(col).collect() } else { String::new() };
            let before: String = text.chars().take(col).collect();
            let trimmed = before.trim_end();
            let after_char = b.char_at(s.end());
            let opens = trimmed.ends_with('{') || trimmed.ends_with('[') || trimmed.ends_with('(') || (python && trimmed.ends_with(':'));
            let closes_next = matches!((trimmed.chars().last(), after_char), (Some('{'), Some('}')) | (Some('['), Some(']')) | (Some('('), Some(')')));
            if auto_indent && opens && closes_next {
                let first = format!("{eol}{indent}{unit}");
                let ins = format!("{first}{eol}{indent}");
                carets.push(first.chars().count());
                edits.push((s.range(), ins));
            } else if auto_indent && opens {
                let ins = format!("{eol}{indent}{unit}");
                carets.push(ins.chars().count());
                edits.push((s.range(), ins));
            } else {
                let ins = format!("{eol}{indent}");
                carets.push(ins.chars().count());
                edits.push((s.range(), ins));
            }
        }
        if self.read_only_guard(cx) {
            return;
        }
        let mut delta: isize = 0;
        let mut sels = Vec::new();
        for ((r, t), off) in edits.iter().zip(carets.iter()) {
            let start = (r.start as isize + delta) as usize;
            sels.push(Selection::caret(start + off));
            delta += t.chars().count() as isize - (r.end - r.start) as isize;
        }
        self.transact(edits, EditKind::Typing, cx);
        self.rect = None;
        self.set_selections(sels, cx);
        self.finish_transaction(cx);
    }

    pub fn newline_above(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let eol = b.eol.as_str();
        let mut edits = Vec::new();
        for s in &self.selections {
            let line = b.char_to_line(s.head);
            let start = b.line_to_char(line);
            let indent: String = b.line_text(line).chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            edits.push((start..start, format!("{indent}{eol}")));
        }
        let lens: Vec<usize> = edits.iter().map(|(_, t)| t.chars().count() - eol.chars().count()).collect();
        let mut delta = 0usize;
        let mut sels = Vec::new();
        for ((r, t), l) in edits.iter().zip(lens) {
            sels.push(Selection::caret(r.start + delta + l));
            delta += t.chars().count();
        }
        if self.read_only_guard(cx) {
            return;
        }
        self.transact(edits, EditKind::Other, cx);
        self.set_selections(sels, cx);
        self.finish_transaction(cx);
    }

    pub fn newline_below(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let eol = b.eol.as_str();
        let mut edits = Vec::new();
        for s in &self.selections {
            let line = b.char_to_line(s.head);
            let end = b.line_end(line);
            let indent: String = b.line_text(line).chars().take_while(|c| *c == ' ' || *c == '\t').collect();
            edits.push((end..end, format!("{eol}{indent}")));
        }
        self.apply_edits_with_carets(edits, false, EditKind::Other, cx);
    }

    pub fn tab(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.completion.is_some() {
            self.completion_confirm(w, cx);
            return;
        }
        if self.mode == EditorMode::SingleLine {
            cx.propagate();
            return;
        }
        let b = self.buffer.read(cx);
        let multi_line = self.selections.iter().any(|s| b.char_to_line(s.start()) != b.char_to_line(s.end()));
        if multi_line {
            self.indent(w, cx);
            return;
        }
        let s = cx.global::<Settings>();
        let tab = s.tab_size.max(1);
        let use_spaces = s.use_spaces;
        let edits: Vec<(Range<usize>, String)> = self
            .selections
            .iter()
            .map(|sel| {
                if use_spaces {
                    let line = b.char_to_line(sel.start());
                    let start = b.line_to_char(line);
                    let before: String = b.slice_string(start..sel.start());
                    let col = crate::tools::tabs_to_spaces(&before, tab).chars().count();
                    (sel.range(), " ".repeat(tab - col % tab))
                } else {
                    (sel.range(), "\t".to_string())
                }
            })
            .collect();
        self.apply_edits_with_carets(edits, false, EditKind::Typing, cx);
    }

    pub fn shift_tab(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            cx.propagate();
            return;
        }
        self.outdent(w, cx);
    }

    /// Line blocks touched by selections: inclusive (first, last) line pairs, merged.
    pub(crate) fn selected_line_blocks(&self, b: &Buffer) -> Vec<(usize, usize)> {
        let mut blocks: Vec<(usize, usize)> = self
            .selections
            .iter()
            .map(|s| {
                let l0 = b.char_to_line(s.start());
                let mut l1 = b.char_to_line(s.end());
                if l1 > l0 && s.end() == b.line_to_char(l1) {
                    l1 -= 1;
                }
                (l0, l1)
            })
            .collect();
        blocks.sort();
        let mut merged: Vec<(usize, usize)> = Vec::new();
        for (a, z) in blocks {
            if let Some(last) = merged.last_mut() {
                if a <= last.1 + 1 {
                    last.1 = last.1.max(z);
                    continue;
                }
            }
            merged.push((a, z));
        }
        merged
    }

    pub fn indent(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let unit = self.indent_unit(cx);
        let b = self.buffer.read(cx);
        let mut edits = Vec::new();
        for (l0, l1) in self.selected_line_blocks(b) {
            for l in l0..=l1 {
                if b.line_len(l) > 0 || l0 == l1 {
                    let p = b.line_to_char(l);
                    edits.push((p..p, unit.clone()));
                }
            }
        }
        self.apply_edits_mapping(edits, cx);
    }

    pub fn outdent(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tab_size(cx);
        let b = self.buffer.read(cx);
        let mut edits = Vec::new();
        for (l0, l1) in self.selected_line_blocks(b) {
            for l in l0..=l1 {
                let text = b.line_text(l);
                let mut n = 0;
                let mut width = 0;
                for c in text.chars() {
                    if c == '\t' {
                        n += 1;
                        break;
                    } else if c == ' ' && width < tab {
                        n += 1;
                        width += 1;
                    } else {
                        break;
                    }
                }
                if n > 0 {
                    let p = b.line_to_char(l);
                    edits.push((p..p + n, String::new()));
                }
            }
        }
        self.apply_edits_mapping(edits, cx);
    }

    // ------------------------------------------------------------------ undo / clipboard

    pub fn undo(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let origin = self.view_id;
        if let Some(sels) = self.buffer.update(cx, |b, cx| b.undo(origin, cx)) {
            self.restore_selections(sels, cx);
        }
    }

    pub fn redo(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let origin = self.view_id;
        if let Some(sels) = self.buffer.update(cx, |b, cx| b.redo(origin, cx)) {
            self.restore_selections(sels, cx);
        }
    }

    fn restore_selections(&mut self, sels: Vec<Selection>, cx: &mut Context<Self>) {
        self.rect = None;
        if sels.is_empty() {
            let len = self.buf(cx).len_chars();
            let s: Vec<Selection> = self.selections.iter().map(|s| Selection::caret(s.head.min(len))).collect();
            self.set_selections(s, cx);
        } else {
            self.set_selections(sels, cx);
        }
    }

    fn clipboard_payload(&self, cx: &gpui::App) -> Option<(String, &'static str, Vec<Range<usize>>)> {
        let b = self.buf(cx);
        if self.has_selection() {
            let eol = b.eol.as_str();
            let parts: Vec<String> = self.selections.iter().filter(|s| !s.is_empty()).map(|s| b.slice_string(s.range())).collect();
            let ranges = self.selections.iter().filter(|s| !s.is_empty()).map(|s| s.range()).collect();
            let tag = if parts.len() > 1 { CLIP_MULTI } else { "" };
            Some((parts.join(eol), tag, ranges))
        } else if self.mode == EditorMode::Full && cx.global::<Settings>().copy_line_without_selection {
            let mut text = String::new();
            let mut ranges = Vec::new();
            let mut seen = std::collections::BTreeSet::new();
            for s in &self.selections {
                let l = b.char_to_line(s.head);
                if !seen.insert(l) {
                    continue;
                }
                let start = b.line_to_char(l);
                let end = if l + 1 < b.len_lines() { b.line_to_char(l + 1) } else { b.len_chars() };
                let mut t = b.slice_string(start..end);
                if l + 1 >= b.len_lines() {
                    t.push_str(b.eol.as_str());
                }
                text.push_str(&t);
                ranges.push(start..end);
            }
            Some((text, CLIP_LINE, ranges))
        } else {
            None
        }
    }

    pub fn copy(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some((text, tag, _)) = self.clipboard_payload(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(text.clone(), tag.to_string()));
            push_clipboard_history(cx, &text);
        }
    }

    pub fn cut(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only_guard(cx) {
            return;
        }
        if let Some((text, tag, ranges)) = self.clipboard_payload(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(text.clone(), tag.to_string()));
            push_clipboard_history(cx, &text);
            let edits = ranges.into_iter().map(|r| (r, String::new())).collect();
            self.apply_edits_with_carets(edits, false, EditKind::Other, cx);
        }
    }

    pub fn paste(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else { return };
        let Some(text) = item.text() else { return };
        let meta = item.metadata().cloned().unwrap_or_default();
        self.paste_text(&text, &meta, w, cx);
    }

    pub fn paste_text(&mut self, text: &str, meta: &str, w: &mut Window, cx: &mut Context<Self>) {
        if self.mode == EditorMode::SingleLine {
            let t = text.replace("\r\n", " ").replace(['\n', '\r'], " ");
            self.insert_text(&t, w, cx);
            return;
        }
        let b = self.buffer.read(cx);
        let eol = b.eol;
        let text = crate::encoding::normalize_eol(text, eol);
        if meta == CLIP_LINE && !self.has_selection() {
            // Whole-line paste goes above the caret line.
            let mut edits = Vec::new();
            for s in &self.selections {
                let l = b.char_to_line(s.head);
                let p = b.line_to_char(l);
                edits.push((p..p, text.clone()));
            }
            self.apply_edits_mapping(edits, cx);
            return;
        }
        let lines: Vec<&str> = text.split(eol.as_str()).collect();
        let n = self.selections.len();
        let edits: Vec<(Range<usize>, String)> = if n > 1 && (lines.len() == n || (lines.len() == n + 1 && lines[n].is_empty())) {
            self.selections.iter().zip(lines.iter()).map(|(s, l)| (s.range(), l.to_string())).collect()
        } else {
            self.selections.iter().map(|s| (s.range(), text.clone())).collect()
        };
        self.apply_edits_with_carets(edits, false, EditKind::Other, cx);
    }

    // ------------------------------------------------------------------ line operations

    /// Replace a block of lines with transformed lines. With no selection the whole document is used
    /// (when `whole_doc`) or the caret line.
    fn line_block_op(&mut self, whole_doc: bool, cx: &mut Context<Self>, f: impl FnOnce(Vec<String>) -> Vec<String>) {
        if self.read_only_guard(cx) {
            return;
        }
        let b = self.buffer.read(cx);
        let p = self.primary_selection();
        let has_sel = self.has_selection();
        let (l0, mut l1) = if has_sel {
            let starts = self.selections.iter().map(|s| s.start()).min().unwrap_or(0);
            let ends = self.selections.iter().map(|s| s.end()).max().unwrap_or(0);
            let l0 = b.char_to_line(starts);
            let mut l1 = b.char_to_line(ends);
            if l1 > l0 && ends == b.line_to_char(l1) {
                l1 -= 1;
            }
            (l0, l1)
        } else if whole_doc {
            (0, b.len_lines().saturating_sub(1))
        } else {
            let l = b.char_to_line(p.head);
            (l, l)
        };
        if !has_sel && whole_doc && l1 > l0 && b.line_len(l1) == 0 {
            l1 -= 1; // keep the final empty line out of the operation
        }
        let start = b.line_to_char(l0);
        let end = b.line_end(l1);
        let eol = b.eol.as_str().to_string();
        let lines: Vec<String> = (l0..=l1).map(|l| b.line_text(l)).collect();
        let old = b.slice_string(start..end);
        let new_lines = f(lines);
        let new = new_lines.join(&eol);
        if new == old {
            return;
        }
        let n = new.chars().count();
        self.transact(vec![(start..end, new)], EditKind::Other, cx);
        self.rect = None;
        if has_sel {
            self.set_selections(vec![Selection::new(start, start + n)], cx);
        } else {
            let caret = p.head.min(start + n);
            self.set_selections(vec![Selection::caret(caret)], cx);
        }
        self.finish_transaction(cx);
    }

    fn line_op(&mut self, op: LineOp, cx: &mut Context<Self>) {
        // Rectangular selection: sort by the selected columns.
        if let (Some(_), true) = (self.rect, is_sort(op)) {
            self.sort_by_columns(op, cx);
            return;
        }
        self.line_block_op(true, cx, |lines| tools::apply_line_op(lines, op));
    }

    fn sort_by_columns(&mut self, op: LineOp, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let mut keyed: Vec<(String, String)> = Vec::new();
        let l0 = b.char_to_line(self.selections.first().map(|s| s.start()).unwrap_or(0));
        let l1 = b.char_to_line(self.selections.last().map(|s| s.end()).unwrap_or(0));
        for l in l0..=l1 {
            let key = self
                .selections
                .iter()
                .find(|s| b.char_to_line(s.start()) == l)
                .map(|s| b.slice_string(s.range()))
                .unwrap_or_default();
            keyed.push((key, b.line_text(l)));
        }
        let keys: Vec<String> = keyed.iter().map(|(k, _)| k.clone()).collect();
        let sorted_keys = tools::apply_line_op(keys, op);
        let mut pool = keyed;
        let mut out = Vec::new();
        for k in sorted_keys {
            if let Some(i) = pool.iter().position(|(pk, _)| *pk == k) {
                out.push(pool.remove(i).1);
            }
        }
        let start = b.line_to_char(l0);
        let end = b.line_end(l1);
        let new = out.join(b.eol.as_str());
        self.transact(vec![(start..end, new)], EditKind::Other, cx);
        self.rect = None;
        self.set_selections(vec![Selection::caret(start)], cx);
        self.finish_transaction(cx);
    }

    pub fn remove_duplicate_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::RemoveDuplicates, cx);
    }
    pub fn remove_adjacent_duplicate_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::RemoveAdjacentDuplicates, cx);
    }
    pub fn remove_empty_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::RemoveEmpty, cx);
    }
    pub fn remove_blank_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::RemoveBlank, cx);
    }
    pub fn sort_lines_lex_asc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortLexAsc, cx);
    }
    pub fn sort_lines_lex_desc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortLexDesc, cx);
    }
    pub fn sort_lines_lex_ci_asc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortLexCiAsc, cx);
    }
    pub fn sort_lines_lex_ci_desc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortLexCiDesc, cx);
    }
    pub fn sort_lines_int_asc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortIntAsc, cx);
    }
    pub fn sort_lines_int_desc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortIntDesc, cx);
    }
    pub fn sort_lines_decimal_dot_asc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortDecimalDotAsc, cx);
    }
    pub fn sort_lines_decimal_dot_desc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortDecimalDotDesc, cx);
    }
    pub fn sort_lines_decimal_comma_asc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortDecimalCommaAsc, cx);
    }
    pub fn sort_lines_decimal_comma_desc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortDecimalCommaDesc, cx);
    }
    pub fn sort_lines_length_asc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortLengthAsc, cx);
    }
    pub fn sort_lines_length_desc(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::SortLengthDesc, cx);
    }
    pub fn reverse_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::Reverse, cx);
    }
    pub fn randomize_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_op(LineOp::Randomize, cx);
    }
    pub fn trim_trailing_space(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_block_op(true, cx, |l| tools::apply_line_op(l, LineOp::TrimTrailing));
    }
    pub fn trim_leading_space(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_block_op(true, cx, |l| tools::apply_line_op(l, LineOp::TrimLeading));
    }
    pub fn trim_leading_and_trailing_space(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_block_op(true, cx, |l| tools::apply_line_op(l, LineOp::TrimBoth));
    }
    pub fn join_lines_with_spaces(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_block_op(true, cx, |l| vec![l.join(" ")]);
    }
    pub fn trim_and_join_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_block_op(true, cx, |l| {
            vec![l.iter().map(|s| s.trim_matches([' ', '\t'])).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ")]
        });
    }
    pub fn tabs_to_spaces(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tab_size(cx);
        self.line_block_op(true, cx, |l| l.iter().map(|s| tools::tabs_to_spaces(s, tab)).collect());
    }
    pub fn spaces_to_tabs_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tab_size(cx);
        self.line_block_op(true, cx, |l| l.iter().map(|s| tools::spaces_to_tabs(s, tab, false)).collect());
    }
    pub fn spaces_to_tabs_leading(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tab_size(cx);
        self.line_block_op(true, cx, |l| l.iter().map(|s| tools::spaces_to_tabs(s, tab, true)).collect());
    }

    pub fn insert_blank_line_above(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let eol = b.eol.as_str().to_string();
        let mut edits = Vec::new();
        for s in &self.selections {
            let p = b.line_to_char(b.char_to_line(s.head));
            edits.push((p..p, eol.clone()));
        }
        self.apply_edits_mapping(edits, cx);
        let _ = w;
    }

    pub fn insert_blank_line_below(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let eol = b.eol.as_str().to_string();
        let mut edits = Vec::new();
        for s in &self.selections {
            let p = b.line_end(b.char_to_line(s.head));
            edits.push((p..p, eol.clone()));
        }
        let old = self.selections.clone();
        if self.read_only_guard(cx) {
            return;
        }
        let infos = self.transact(edits, EditKind::Other, cx);
        let sels: Vec<Selection> = old.iter().map(|s| Selection::new(map_pos(s.anchor, &infos, false), map_pos(s.head, &infos, false))).collect();
        self.set_selections(sels, cx);
        self.finish_transaction(cx);
    }

    pub fn duplicate_line(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let eol = b.eol.as_str().to_string();
        let mut edits = Vec::new();
        if self.has_selection() {
            for s in &self.selections {
                let t = b.slice_string(s.range());
                edits.push((s.end()..s.end(), t));
            }
        } else {
            for (l0, l1) in self.selected_line_blocks(b) {
                let start = b.line_to_char(l0);
                let end = b.line_end(l1);
                let t = b.slice_string(start..end);
                edits.push((end..end, format!("{eol}{t}")));
            }
        }
        let old = self.selections.clone();
        if self.read_only_guard(cx) {
            return;
        }
        let infos = self.transact(edits, EditKind::Other, cx);
        let sels: Vec<Selection> = old.iter().map(|s| Selection::new(map_pos(s.anchor, &infos, false), map_pos(s.head, &infos, false))).collect();
        self.set_selections(sels, cx);
        self.finish_transaction(cx);
    }

    pub fn delete_line(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let n = b.len_lines();
        let mut edits = Vec::new();
        for (l0, l1) in self.selected_line_blocks(b) {
            let (start, end) = if l1 + 1 < n {
                (b.line_to_char(l0), b.line_to_char(l1 + 1))
            } else if l0 > 0 {
                (b.line_end(l0 - 1), b.len_chars())
            } else {
                (0, b.len_chars())
            };
            edits.push((start..end, String::new()));
        }
        self.apply_edits_with_carets(edits, false, EditKind::Other, cx);
    }

    fn move_lines(&mut self, up: bool, cx: &mut Context<Self>) {
        if self.read_only_guard(cx) {
            return;
        }
        let b = self.buffer.read(cx);
        let n = b.len_lines();
        let eol = b.eol.as_str().to_string();
        let blocks = self.selected_line_blocks(b);
        if (up && blocks.first().map(|b| b.0 == 0).unwrap_or(true)) || (!up && blocks.last().map(|b| b.1 + 1 >= n).unwrap_or(true)) {
            return;
        }
        let mut edits = Vec::new();
        let mut shifts: Vec<(Range<usize>, isize)> = Vec::new();
        for (l0, l1) in blocks {
            if up {
                let start = b.line_to_char(l0 - 1);
                let end = b.line_end(l1);
                let above = b.line_text(l0 - 1);
                let block: Vec<String> = (l0..=l1).map(|l| b.line_text(l)).collect();
                let new = format!("{}{eol}{above}", block.join(&eol));
                let shift = -((above.chars().count() + eol.chars().count()) as isize);
                shifts.push((b.line_to_char(l0)..end + 1, shift));
                edits.push((start..end, new));
            } else {
                let start = b.line_to_char(l0);
                let end = b.line_end(l1 + 1);
                let below = b.line_text(l1 + 1);
                let block: Vec<String> = (l0..=l1).map(|l| b.line_text(l)).collect();
                let new = format!("{below}{eol}{}", block.join(&eol));
                let shift = (below.chars().count() + eol.chars().count()) as isize;
                shifts.push((start..b.line_end(l1) + 1, shift));
                edits.push((start..end, new));
            }
        }
        let old = self.selections.clone();
        self.transact(edits, EditKind::Other, cx);
        let sels: Vec<Selection> = old
            .iter()
            .map(|s| {
                let sh = shifts.iter().find(|(r, _)| r.contains(&s.head)).map(|(_, d)| *d).unwrap_or(0);
                Selection::new((s.anchor as isize + sh).max(0) as usize, (s.head as isize + sh).max(0) as usize)
            })
            .collect();
        self.set_selections(sels, cx);
        self.finish_transaction(cx);
    }

    pub fn move_line_up(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_lines(true, cx);
    }
    pub fn move_line_down(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.move_lines(false, cx);
    }

    pub fn transpose_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let l = b.char_to_line(self.caret());
        if l == 0 {
            return;
        }
        let start = b.line_to_char(l - 1);
        let end = b.line_end(l);
        let new = format!("{}{}{}", b.line_text(l), b.eol.as_str(), b.line_text(l - 1));
        let caret = self.caret();
        if self.read_only_guard(cx) {
            return;
        }
        self.transact(vec![(start..end, new)], EditKind::Other, cx);
        self.set_selections(vec![Selection::caret(caret)], cx);
        self.finish_transaction(cx);
    }

    pub fn join_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.read_only_guard(cx) {
            return;
        }
        let b = self.buffer.read(cx);
        let p = self.primary_selection();
        let l0 = b.char_to_line(p.start());
        let mut l1 = b.char_to_line(p.end());
        if l1 > l0 && p.end() == b.line_to_char(l1) {
            l1 -= 1;
        }
        if l1 == l0 {
            if l0 + 1 >= b.len_lines() {
                return;
            }
            l1 = l0 + 1;
        }
        let start = b.line_to_char(l0);
        let end = b.line_end(l1);
        let mut out = String::new();
        for l in l0..=l1 {
            let t = b.line_text(l);
            if l == l0 {
                out.push_str(t.trim_end());
            } else {
                let t = t.trim();
                if !t.is_empty() {
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(t);
                }
            }
        }
        let n = out.chars().count();
        self.transact(vec![(start..end, out)], EditKind::Other, cx);
        self.set_selections(vec![Selection::caret(start + n)], cx);
        self.finish_transaction(cx);
    }

    pub fn split_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let width = cx.global::<Settings>().edge_column.max(10);
        self.line_block_op(false, cx, |lines| {
            let mut out = Vec::new();
            for l in lines {
                let indent: String = l.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
                let mut cur = String::new();
                for word in l.split_whitespace() {
                    if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
                        out.push(cur);
                        cur = String::new();
                    }
                    if cur.is_empty() {
                        cur.push_str(&indent);
                        cur.push_str(word);
                    } else {
                        cur.push(' ');
                        cur.push_str(word);
                    }
                }
                out.push(cur);
            }
            out
        });
    }

    pub fn toggle_overwrite(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.overwrite = !self.overwrite;
        cx.emit(EditorEvent::SelectionsChanged);
        cx.notify();
    }

    // ------------------------------------------------------------------ transforms of selected text

    /// Transform each selection's text (or the word at the caret / whole document).
    pub(crate) fn transform_selections(
        &mut self,
        empty_means_doc: bool,
        cx: &mut Context<Self>,
        f: impl Fn(&str) -> Result<String, String>,
    ) {
        let b = self.buffer.read(cx);
        let mut edits = Vec::new();
        let ranges: Vec<Range<usize>> = if self.has_selection() {
            self.selections.iter().filter(|s| !s.is_empty()).map(|s| s.range()).collect()
        } else if empty_means_doc {
            vec![0..b.len_chars()]
        } else {
            self.selections.iter().filter_map(|s| word_at(b, s.head)).collect()
        };
        for r in ranges {
            let old = b.slice_string(r.clone());
            match f(&old) {
                Ok(new) => {
                    if new != old {
                        edits.push((r, new));
                    }
                }
                Err(e) => {
                    cx.emit(EditorEvent::Notify(e));
                    return;
                }
            }
        }
        if !edits.is_empty() {
            self.apply_edits_with_carets(edits, true, EditKind::Other, cx);
        }
    }

    fn case(&mut self, mode: CaseMode, cx: &mut Context<Self>) {
        self.transform_selections(false, cx, |s| Ok(tools::convert_case(s, mode)));
    }

    pub fn upper_case(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.case(CaseMode::Upper, cx);
    }
    pub fn lower_case(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.case(CaseMode::Lower, cx);
    }
    pub fn title_case(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.case(CaseMode::Title, cx);
    }
    pub fn title_case_keep_caps(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.case(CaseMode::TitleKeepCaps, cx);
    }
    pub fn sentence_case(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.case(CaseMode::Sentence, cx);
    }
    pub fn sentence_case_keep_caps(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.case(CaseMode::SentenceKeepCaps, cx);
    }
    pub fn invert_case(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.case(CaseMode::Invert, cx);
    }
    pub fn random_case(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.case(CaseMode::Random, cx);
    }

    pub fn base64_encode(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.transform_selections(true, cx, |s| Ok(tools::base64_encode(s)));
    }
    pub fn base64_decode(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.transform_selections(true, cx, |s| tools::base64_decode(s).map_err(|e| format!("Base64 decode failed: {e}")));
    }
    pub fn url_encode(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.transform_selections(true, cx, |s| Ok(tools::url_encode(s)));
    }
    pub fn url_decode(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.transform_selections(true, cx, |s| Ok(tools::url_decode(s)));
    }
    pub fn ascii_to_hex(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.transform_selections(true, cx, |s| Ok(tools::ascii_to_hex(s)));
    }
    pub fn hex_to_ascii(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.transform_selections(true, cx, tools::hex_to_ascii);
    }
    pub fn json_format(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tab_size(cx);
        self.transform_selections(true, cx, |s| tools::json_format(s, tab).map_err(|e| format!("Invalid JSON: {e}")));
    }
    pub fn json_minify(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.transform_selections(true, cx, |s| tools::json_minify(s).map_err(|e| format!("Invalid JSON: {e}")));
    }
    pub fn xml_pretty_print(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let tab = self.tab_size(cx);
        self.transform_selections(true, cx, |s| Ok(tools::xml_pretty(s, tab)));
    }

    // ------------------------------------------------------------------ comments

    fn comment_tokens(&self, cx: &gpui::App) -> (Option<&'static str>, Option<(&'static str, &'static str)>) {
        crate::syntax::comment_tokens(&self.buf(cx).syntax_name)
    }

    fn line_comment_impl(&mut self, mode: i8, cx: &mut Context<Self>) {
        // mode: 0 toggle, 1 comment, -1 uncomment
        let (line_tok, block) = self.comment_tokens(cx);
        let Some(tok) = line_tok else {
            if let Some((bs, be)) = block {
                self.block_comment_impl(bs, be, mode >= 0, cx);
            }
            return;
        };
        let tab = self.tab_size(cx);
        let b = self.buffer.read(cx);
        let tok_trim = tok.trim_end();
        let mut edits = Vec::new();
        for (l0, l1) in self.selected_line_blocks(b) {
            let lines: Vec<(usize, String)> = (l0..=l1).map(|l| (l, b.line_text(l))).collect();
            let nonblank: Vec<&(usize, String)> = lines.iter().filter(|(_, t)| !t.trim().is_empty()).collect();
            if nonblank.is_empty() {
                continue;
            }
            let all_commented = nonblank.iter().all(|(_, t)| t.trim_start().starts_with(tok_trim));
            let uncomment = match mode {
                0 => all_commented,
                1 => false,
                _ => true,
            };
            if uncomment {
                for (l, t) in &nonblank {
                    let lead = t.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                    let rest: String = t.chars().skip(lead).collect();
                    if rest.starts_with(tok_trim) {
                        let mut n = tok_trim.chars().count();
                        if rest.chars().nth(n) == Some(' ') {
                            n += 1;
                        }
                        let p = b.line_to_char(*l) + lead;
                        edits.push((p..p + n, String::new()));
                    }
                }
            } else {
                let min_indent = nonblank
                    .iter()
                    .map(|(_, t)| display::line_indent_chars(t, tab))
                    .min()
                    .unwrap_or(0);
                for (l, t) in &nonblank {
                    // Convert the visual min indent to a char offset on this line.
                    let off = display::char_offset_for_indent(t, min_indent, tab);
                    let p = b.line_to_char(*l) + off;
                    edits.push((p..p, format!("{tok_trim} ")));
                }
            }
        }
        self.apply_edits_mapping(edits, cx);
    }

    pub fn toggle_line_comment(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_comment_impl(0, cx);
    }
    pub fn line_comment(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_comment_impl(1, cx);
    }
    pub fn line_uncomment(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.line_comment_impl(-1, cx);
    }

    fn block_comment_impl(&mut self, bs: &str, be: &str, comment: bool, cx: &mut Context<Self>) {
        let b = self.buffer.read(cx);
        let mut edits = Vec::new();
        for s in &self.selections {
            let r = if s.is_empty() {
                let l = b.char_to_line(s.head);
                let start = b.line_to_char(l);
                let text = b.line_text(l);
                let lead = text.chars().take_while(|c| *c == ' ' || *c == '\t').count();
                start + lead..b.line_end(l)
            } else {
                s.range()
            };
            let text = b.slice_string(r.clone());
            if comment {
                edits.push((r, format!("{bs}{text}{be}")));
            } else {
                let t = text.trim();
                if t.starts_with(bs) && t.ends_with(be) && t.len() >= bs.len() + be.len() {
                    let lead = text.len() - text.trim_start().len();
                    let inner = &t[bs.len()..t.len() - be.len()];
                    let new = format!("{}{}{}", &text[..lead], inner, &text[lead + t.len()..]);
                    edits.push((r, new));
                } else {
                    // Search around the caret for an enclosing comment.
                    let caret = s.head;
                    let from = caret.saturating_sub(20_000);
                    let to = (caret + 20_000).min(b.len_chars());
                    let around = b.slice_string(from..to);
                    let rel = caret - from;
                    let rel_b: usize = around.chars().take(rel).map(|c| c.len_utf8()).sum();
                    if let (Some(open), Some(close)) = (around[..rel_b].rfind(bs), around[rel_b..].find(be)) {
                        let open_c = from + around[..open].chars().count();
                        let close_c = caret + around[rel_b..rel_b + close].chars().count();
                        edits.push((close_c..close_c + be.chars().count(), String::new()));
                        edits.push((open_c..open_c + bs.chars().count(), String::new()));
                    }
                }
            }
        }
        self.apply_edits_mapping(edits, cx);
    }

    pub fn block_comment(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match self.comment_tokens(cx) {
            (_, Some((bs, be))) => self.block_comment_impl(bs, be, true, cx),
            (Some(_), None) => self.line_comment_impl(1, cx),
            _ => {}
        }
    }

    pub fn block_uncomment(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match self.comment_tokens(cx) {
            (_, Some((bs, be))) => self.block_comment_impl(bs, be, false, cx),
            (Some(_), None) => self.line_comment_impl(-1, cx),
            _ => {}
        }
    }

    // ------------------------------------------------------------------ date / time

    fn insert_formatted_now(&mut self, fmt: &str, w: &mut Window, cx: &mut Context<Self>) {
        let now = chrono::Local::now();
        let mut s = String::new();
        use std::fmt::Write as _;
        if write!(s, "{}", now.format(fmt)).is_err() {
            s = now.to_rfc2822();
        }
        let edits = self.selections.iter().map(|sel| (sel.range(), s.clone())).collect();
        self.apply_edits_with_carets(edits, false, EditKind::Other, cx);
        let _ = w;
    }

    pub fn insert_date_time_short(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.insert_formatted_now("%-I:%M %p %-m/%-d/%Y", w, cx);
    }
    pub fn insert_date_time_long(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.insert_formatted_now("%-I:%M %p %A, %B %-d, %Y", w, cx);
    }
    pub fn insert_date_time_custom(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let fmt = cx.global::<Settings>().date_time_format.clone();
        self.insert_formatted_now(&fmt, w, cx);
    }

    // ------------------------------------------------------------------ bookmarks

    pub fn toggle_bookmark(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buf(cx);
        let mut lines: Vec<usize> = self.selections.iter().map(|s| b.char_to_line(s.head)).collect();
        lines.dedup();
        self.buffer.update(cx, |b, cx| {
            for l in lines {
                if !b.bookmarks.remove(&l) {
                    b.bookmarks.insert(l);
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    pub fn toggle_bookmark_at_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.buffer.update(cx, |b, cx| {
            if !b.bookmarks.remove(&line) {
                b.bookmarks.insert(line);
            }
            cx.notify();
        });
        cx.notify();
    }

    pub fn next_bookmark(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buf(cx);
        let cur = b.char_to_line(self.caret());
        let target = b.bookmarks.range(cur + 1..).next().copied().or_else(|| b.bookmarks.iter().next().copied());
        if let Some(l) = target {
            self.go_to_line(l, cx);
        }
    }

    pub fn prev_bookmark(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buf(cx);
        let cur = b.char_to_line(self.caret());
        let target = b.bookmarks.range(..cur).next_back().copied().or_else(|| b.bookmarks.iter().next_back().copied());
        if let Some(l) = target {
            self.go_to_line(l, cx);
        }
    }

    pub fn clear_bookmarks(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.update(cx, |b, cx| {
            b.bookmarks.clear();
            cx.notify();
        });
        cx.notify();
    }

    fn bookmarked_line_ranges(&self, cx: &gpui::App, invert: bool) -> Vec<Range<usize>> {
        let b = self.buf(cx);
        let n = b.len_lines();
        // The empty "line" after a final line break is not a real line.
        let real = if n > 1 && b.line_len(n - 1) == 0 { n - 1 } else { n };
        let lines: Vec<usize> = if invert {
            (0..real).filter(|l| !b.bookmarks.contains(l)).collect()
        } else {
            b.bookmarks.iter().copied().filter(|l| *l < n).collect()
        };
        lines
            .into_iter()
            .map(|l| {
                if l + 1 < n {
                    b.line_to_char(l)..b.line_to_char(l + 1)
                } else if l > 0 {
                    b.line_end(l - 1)..b.len_chars()
                } else {
                    0..b.len_chars()
                }
            })
            .collect()
    }

    pub fn copy_bookmarked_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buf(cx);
        let eol = b.eol.as_str();
        let text: String = b
            .bookmarks
            .iter()
            .filter(|l| **l < b.len_lines())
            .map(|l| format!("{}{eol}", b.line_text(*l)))
            .collect();
        if !text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            push_clipboard_history(cx, &text);
        }
    }

    pub fn cut_bookmarked_lines(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.copy_bookmarked_lines(w, cx);
        self.remove_bookmarked_lines(w, cx);
    }

    pub fn remove_bookmarked_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let ranges = self.bookmarked_line_ranges(cx, false);
        let edits: Vec<_> = ranges.into_iter().map(|r| (r, String::new())).collect();
        self.apply_edits_mapping(edits, cx);
        self.buffer.update(cx, |b, _| b.bookmarks.clear());
    }

    pub fn keep_only_bookmarked_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let ranges = self.bookmarked_line_ranges(cx, true);
        let edits: Vec<_> = ranges.into_iter().map(|r| (r, String::new())).collect();
        self.apply_edits_mapping(edits, cx);
    }

    pub fn replace_bookmarked_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) else { return };
        let b = self.buf(cx);
        let text = crate::encoding::normalize_eol(text.trim_end_matches(['\r', '\n']), b.eol);
        let edits: Vec<_> = b
            .bookmarks
            .iter()
            .filter(|l| **l < b.len_lines())
            .map(|l| (b.line_to_char(*l)..b.line_end(*l), text.clone()))
            .collect();
        self.apply_edits_mapping(edits, cx);
    }

    pub fn invert_bookmarks(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.update(cx, |b, cx| {
            let n = b.len_lines();
            b.bookmarks = (0..n).filter(|l| !b.bookmarks.contains(l)).collect();
            cx.notify();
        });
        cx.notify();
    }

    // ------------------------------------------------------------------ folding

    pub fn toggle_fold_line(&mut self, line: usize, cx: &mut Context<Self>) {
        if self.folds.remove(&line).is_some() {
            self.display.invalidate_folds();
            cx.notify();
            return;
        }
        let tab = self.tab_size(cx);
        if let Some(end) = display::fold_region(self.buf(cx), line, tab, 1_000_000) {
            self.folds.insert(line, end);
            self.display.invalidate_folds();
            // Move carets out of the folded region.
            let b = self.buf(cx);
            let hs = b.line_to_char(line + 1);
            let he = b.line_end(end);
            let header_end = b.line_end(line);
            if self.selections.iter().any(|s| s.head >= hs && s.head <= he) {
                self.set_selections(vec![Selection::caret(header_end)], cx);
            }
            cx.notify();
        }
    }

    fn enclosing_fold_header(&self, line: usize, cx: &gpui::App) -> Option<usize> {
        let tab = self.tab_size(cx);
        let b = self.buf(cx);
        if display::fold_region(b, line, tab, 1_000_000).is_some() {
            return Some(line);
        }
        let ind = (0..=line).rev().find_map(|l| display::line_indent(&b.line_text(l), tab))?;
        let mut l = line;
        while l > 0 {
            l -= 1;
            if let Some(i) = display::line_indent(&b.line_text(l), tab) {
                if i < ind {
                    return Some(l);
                }
            }
        }
        None
    }

    pub fn toggle_fold_at_caret(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let line = self.buf(cx).char_to_line(self.caret());
        if let Some(h) = self.folds.keys().copied().find(|h| *h == line) {
            self.toggle_fold_line(h, cx);
        } else if let Some(h) = self.enclosing_fold_header(line, cx) {
            self.toggle_fold_line(h, cx);
        }
    }

    pub fn fold_current(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let line = self.buf(cx).char_to_line(self.caret());
        if let Some(h) = self.enclosing_fold_header(line, cx) {
            if !self.folds.contains_key(&h) {
                self.toggle_fold_line(h, cx);
            }
        }
    }

    pub fn unfold_current(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let line = self.buf(cx).char_to_line(self.caret());
        if self.folds.remove(&line).is_some() {
            self.display.invalidate_folds();
            cx.notify();
        } else if let Some(h) = self.enclosing_fold_header(line, cx) {
            if self.folds.remove(&h).is_some() {
                self.display.invalidate_folds();
                cx.notify();
            }
        }
    }

    pub fn fold_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.fold_levels(None, true, cx);
    }

    pub fn unfold_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.folds.clear();
        self.display.invalidate_folds();
        cx.notify();
    }

    fn fold_levels(&mut self, level: Option<usize>, fold: bool, cx: &mut Context<Self>) {
        let tab = self.tab_size(cx);
        let regions = display::all_fold_regions(self.buf(cx), tab);
        for (h, e, depth) in regions {
            if level.map(|l| l == depth).unwrap_or(true) {
                if fold {
                    self.folds.insert(h, e);
                } else {
                    self.folds.remove(&h);
                }
            }
        }
        self.display.invalidate_folds();
        // Keep the caret on a visible line.
        let b = self.buf(cx);
        let line = b.char_to_line(self.caret());
        if let Some((h, _)) = self.folds.iter().find(|(h, e)| **h < line && line <= **e) {
            let p = b.line_end(*h);
            self.set_selections(vec![Selection::caret(p)], cx);
        }
        cx.notify();
    }

    pub fn fold_level_1(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(1), true, cx) }
    pub fn fold_level_2(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(2), true, cx) }
    pub fn fold_level_3(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(3), true, cx) }
    pub fn fold_level_4(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(4), true, cx) }
    pub fn fold_level_5(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(5), true, cx) }
    pub fn fold_level_6(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(6), true, cx) }
    pub fn fold_level_7(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(7), true, cx) }
    pub fn fold_level_8(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(8), true, cx) }
    pub fn unfold_level_1(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(1), false, cx) }
    pub fn unfold_level_2(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(2), false, cx) }
    pub fn unfold_level_3(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(3), false, cx) }
    pub fn unfold_level_4(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(4), false, cx) }
    pub fn unfold_level_5(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(5), false, cx) }
    pub fn unfold_level_6(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(6), false, cx) }
    pub fn unfold_level_7(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(7), false, cx) }
    pub fn unfold_level_8(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.fold_levels(Some(8), false, cx) }

    pub fn hide_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buf(cx);
        let blocks = self.selected_line_blocks(b);
        let first_line = blocks.first().map(|b| b.0).unwrap_or(0);
        self.buffer.update(cx, |b, cx| {
            for (l0, l1) in blocks {
                b.hidden_ranges.push(l0..l1 + 1);
            }
            cx.notify();
        });
        self.display.invalidate_folds();
        let b = self.buf(cx);
        let target = if first_line > 0 { b.line_end(first_line - 1) } else { 0 };
        self.set_selections(vec![Selection::caret(target)], cx);
        // Make sure line 0 hidden case still leaves something visible.
        cx.notify();
    }

    pub fn unhide_all_lines(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.update(cx, |b, cx| {
            b.hidden_ranges.clear();
            cx.notify();
        });
        self.display.invalidate_folds();
        cx.notify();
    }

    pub fn unhide_range_after(&mut self, line: usize, cx: &mut Context<Self>) {
        self.buffer.update(cx, |b, cx| {
            b.hidden_ranges.retain(|r| r.start != line + 1 && !(r.start == 0 && line == 0));
            cx.notify();
        });
        self.display.invalidate_folds();
        cx.notify();
    }

    // ------------------------------------------------------------------ token styles / marks

    fn highlight_token(&mut self, style: u8, all: bool, cx: &mut Context<Self>) {
        let b = self.buf(cx);
        let p = self.primary_selection();
        let (range, whole_word) = if !p.is_empty() {
            (p.range(), false)
        } else if let Some(r) = word_at(b, p.head) {
            (r, true)
        } else {
            return;
        };
        let needle = b.slice_string(range.clone());
        if needle.is_empty() {
            return;
        }
        let mut new_marks = Vec::new();
        if all {
            let text = self.buffer.update(cx, |b, _| b.text_arc());
            let b = self.buf(cx);
            let opts = crate::search::SearchOptions { match_case: true, whole_word, ..Default::default() };
            if let Ok(q) = crate::search::SearchQuery::new(&needle, &opts) {
                for m in q.find_all(&text, 0..text.len(), None) {
                    let s = b.byte_to_char(m.start);
                    let e = b.byte_to_char(m.end);
                    new_marks.push(Mark { range: s..e, style });
                }
            }
        } else {
            new_marks.push(Mark { range, style });
        }
        self.buffer.update(cx, |b, cx| {
            b.marks.retain(|m| !(m.style == style && new_marks.iter().any(|n| n.range == m.range)));
            b.marks.extend(new_marks);
            b.marks.sort_by_key(|m| m.range.start);
            cx.notify();
        });
        cx.notify();
    }

    pub fn highlight_all_1(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(1, true, cx) }
    pub fn highlight_all_2(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(2, true, cx) }
    pub fn highlight_all_3(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(3, true, cx) }
    pub fn highlight_all_4(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(4, true, cx) }
    pub fn highlight_all_5(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(5, true, cx) }
    pub fn highlight_one_1(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(1, false, cx) }
    pub fn highlight_one_2(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(2, false, cx) }
    pub fn highlight_one_3(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(3, false, cx) }
    pub fn highlight_one_4(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(4, false, cx) }
    pub fn highlight_one_5(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.highlight_token(5, false, cx) }

    fn clear_highlight(&mut self, style: Option<u8>, cx: &mut Context<Self>) {
        self.buffer.update(cx, |b, cx| {
            match style {
                Some(s) => b.marks.retain(|m| m.style != s),
                None => b.marks.retain(|m| m.style == 0),
            }
            cx.notify();
        });
        cx.notify();
    }

    pub fn clear_highlight_1(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.clear_highlight(Some(1), cx) }
    pub fn clear_highlight_2(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.clear_highlight(Some(2), cx) }
    pub fn clear_highlight_3(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.clear_highlight(Some(3), cx) }
    pub fn clear_highlight_4(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.clear_highlight(Some(4), cx) }
    pub fn clear_highlight_5(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.clear_highlight(Some(5), cx) }
    pub fn clear_all_highlights(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.clear_highlight(None, cx) }
    pub fn clear_find_marks(&mut self, _: &mut Window, cx: &mut Context<Self>) { self.clear_highlight(Some(0), cx) }

    pub fn jump_to_next_mark(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let caret = self.primary_selection().end();
        let b = self.buf(cx);
        let target = b.marks.iter().filter(|m| m.range.start >= caret && m.range.start != self.primary_selection().start()).min_by_key(|m| m.range.start).or_else(|| b.marks.iter().min_by_key(|m| m.range.start)).map(|m| m.range.clone());
        if let Some(r) = target {
            self.select_range(r, true, cx);
        }
    }

    pub fn jump_to_prev_mark(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let caret = self.primary_selection().start();
        let b = self.buf(cx);
        let target = b.marks.iter().filter(|m| m.range.start < caret).max_by_key(|m| m.range.start).or_else(|| b.marks.iter().max_by_key(|m| m.range.start)).map(|m| m.range.clone());
        if let Some(r) = target {
            self.select_range(r, true, cx);
        }
    }

    pub fn copy_highlighted_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let b = self.buf(cx);
        let eol = b.eol.as_str();
        let mut marks: Vec<&Mark> = b.marks.iter().collect();
        marks.sort_by_key(|m| m.range.start);
        let text: Vec<String> = marks.iter().map(|m| b.slice_string(m.range.clone())).collect();
        if !text.is_empty() {
            let t = text.join(eol);
            cx.write_to_clipboard(ClipboardItem::new_string(t.clone()));
            push_clipboard_history(cx, &t);
        }
    }

    // ------------------------------------------------------------------ completion

    fn current_word_start(&self, cx: &gpui::App) -> usize {
        let b = self.buf(cx);
        let mut p = self.caret();
        let line_start = b.line_to_char(b.char_to_line(p));
        while p > line_start && b.char_at(p - 1).map(crate::tools::is_word_char).unwrap_or(false) {
            p -= 1;
        }
        p
    }

    fn collect_words(&self, cx: &gpui::App, exclude_pos: usize) -> Vec<String> {
        let b = self.buf(cx);
        let len = b.len_chars();
        let caret = self.caret();
        let from = caret.saturating_sub(1_000_000);
        let to = (caret + 1_000_000).min(len);
        let text = b.slice_string(from..to);
        let mut set = std::collections::BTreeSet::new();
        let mut cur = String::new();
        let mut pos = from;
        let mut cur_start = from;
        for c in text.chars() {
            if crate::tools::is_word_char(c) {
                if cur.is_empty() {
                    cur_start = pos;
                }
                cur.push(c);
            } else {
                if cur.chars().count() >= 2 && cur_start != exclude_pos && !cur.chars().next().unwrap().is_ascii_digit() {
                    set.insert(std::mem::take(&mut cur));
                }
                cur.clear();
            }
            pos += 1;
        }
        if cur.chars().count() >= 2 && cur_start != exclude_pos {
            set.insert(cur);
        }
        for k in crate::syntax::keywords(&b.syntax_name) {
            set.insert(k.to_string());
        }
        set.into_iter().collect()
    }

    pub(crate) fn open_completion(&mut self, explicit: bool, cx: &mut Context<Self>) {
        if self.selections.len() != 1 || self.has_selection() {
            self.completion = None;
            return;
        }
        let min = cx.global::<Settings>().auto_completion_min_chars.max(1);
        let ws = self.current_word_start(cx);
        let caret = self.caret();
        let prefix = self.buf(cx).slice_string(ws..caret);
        if prefix.chars().count() < if explicit { 0 } else { min } {
            self.completion = None;
            return;
        }
        let words = Rc::new(self.collect_words(cx, ws));
        let items = filter_words(&words, &prefix);
        if items.is_empty() {
            self.completion = None;
        } else {
            COMPLETER.with(|c| *c.borrow_mut() = Some(Completer { words }));
            self.completion = Some(Completion { items, selected: 0, word_start: ws });
        }
        cx.notify();
    }

    pub(crate) fn completion_refilter(&mut self, cx: &mut Context<Self>) {
        let Some(c) = &self.completion else { return };
        let caret = self.caret();
        if self.selections.len() != 1 || caret < c.word_start || self.has_selection() {
            self.completion = None;
            return;
        }
        let b = self.buf(cx);
        let line_ok = b.char_to_line(caret) == b.char_to_line(c.word_start);
        let prefix = b.slice_string(c.word_start..caret);
        if !line_ok || !prefix.chars().all(crate::tools::is_word_char) {
            self.completion = None;
            return;
        }
        let words = COMPLETER.with(|cm| cm.borrow().as_ref().map(|c| c.words.clone()));
        let Some(words) = words else {
            self.completion = None;
            return;
        };
        let items = filter_words(&words, &prefix);
        if items.is_empty() || prefix.is_empty() && cx.global::<Settings>().auto_completion_min_chars > 0 && false {
            self.completion = None;
        } else if let Some(c) = &mut self.completion {
            c.selected = c.selected.min(items.len().saturating_sub(1));
            c.items = items;
        }
    }

    pub fn trigger_completion(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.open_completion(true, cx);
    }

    pub fn completion_next(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = &mut self.completion {
            c.selected = (c.selected + 1) % c.items.len().max(1);
            cx.notify();
        }
    }

    pub fn completion_prev(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = &mut self.completion {
            let n = c.items.len().max(1);
            c.selected = (c.selected + n - 1) % n;
            cx.notify();
        }
    }

    pub fn completion_confirm(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.completion.take() else { return };
        let Some(word) = c.items.get(c.selected).cloned() else { return };
        let caret = self.caret();
        // Replace the whole identifier under the caret.
        let b = self.buf(cx);
        let mut end = caret;
        while b.char_at(end).map(crate::tools::is_word_char).unwrap_or(false) {
            end += 1;
        }
        self.apply_edits_with_carets(vec![(c.word_start..end, word)], false, EditKind::Other, cx);
        self.completion = None;
        cx.notify();
    }

    pub(crate) fn render_completion(&mut self, _window: &mut Window, cx: &mut Context<Self>, theme: &AppTheme) -> Option<AnyElement> {
        if !self.focused {
            return None;
        }
        let c = self.completion.as_ref()?;
        let layout = self.layout.as_ref()?;
        let caret = self.caret();
        let row = layout.rows.iter().find(|r| {
            let s = r.shape.start_char;
            caret >= s && caret <= s + r.shape.len_chars && r.shape.seg_for_char(caret - s) == r.sub
        })?;
        let (_, x) = row.shape.x_for_char(caret - row.shape.start_char);
        let origin = gpui::point(
            layout.text_bounds.origin.x + px(x - self.scroll_x - 4.0),
            px(row.y + layout.line_height),
        );
        let visible = 10usize;
        let first = c.selected.saturating_sub(visible - 1);
        let entity = cx.entity();
        let items: Vec<AnyElement> = c
            .items
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .map(|(i, item)| {
                let selected = i == c.selected;
                let entity = entity.clone();
                div()
                    .id(("completion", i))
                    .px_2()
                    .py(px(1.0))
                    .text_color(if selected { theme.accent_fg } else { theme.panel_fg })
                    .when(selected, |d| d.bg(theme.accent))
                    .hover(|d| d.bg(theme.list_hover))
                    .child(item.clone())
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        entity.update(cx, |e, cx| {
                            if let Some(c) = &mut e.completion {
                                c.selected = i;
                            }
                            e.completion_confirm(window, cx);
                        });
                    })
                    .into_any_element()
            })
            .collect();
        let font = cx.global::<Settings>().font_family.clone();
        Some(
            deferred(
                anchored().position(origin).child(
                    div()
                        .min_w(px(160.0))
                        .max_w(px(420.0))
                        .bg(theme.panel_bg)
                        .border_1()
                        .border_color(theme.chrome_border)
                        .shadow_md()
                        .rounded(px(3.0))
                        .py(px(2.0))
                        .font_family(font)
                        .text_size(px(12.5))
                        .children(items),
                ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

thread_local! {
    static COMPLETER: std::cell::RefCell<Option<Completer>> = const { std::cell::RefCell::new(None) };
}

fn filter_words(words: &[String], prefix: &str) -> Vec<String> {
    let lower = prefix.to_lowercase();
    let mut items: Vec<String> = words
        .iter()
        .filter(|w| w.to_lowercase().starts_with(&lower) && w.as_str() != prefix)
        .take(500)
        .cloned()
        .collect();
    items.sort_by(|a, b| {
        let ap = a.starts_with(prefix);
        let bp = b.starts_with(prefix);
        bp.cmp(&ap).then(a.len().cmp(&b.len())).then(a.cmp(b))
    });
    items.truncate(200);
    items
}

fn is_sort(op: LineOp) -> bool {
    !matches!(
        op,
        LineOp::RemoveDuplicates
            | LineOp::RemoveAdjacentDuplicates
            | LineOp::RemoveEmpty
            | LineOp::RemoveBlank
            | LineOp::TrimTrailing
            | LineOp::TrimLeading
            | LineOp::TrimBoth
    )
}

use super::display;
