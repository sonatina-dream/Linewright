//! The custom GPUI element that lays out and paints an [`Editor`], and its mouse handling.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    fill, point, px, size, App, Bounds, ContentMask, Corners, CursorStyle, DispatchPhase, Element, ElementId,
    ElementInputHandler, Entity, GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point, ScrollWheelEvent,
    ShapedLine, SharedString, Style, TextRun, Window,
};

use super::display::{self, SpecialKind};
use super::movement::{brace_match, word_range_at};
use super::{
    Autoscroll, DragMode, DragState, Editor, EditorEvent, EditorMode, LayoutSnapshot, RectSelection, RowInfo,
    ScrollbarInfo,
};
use crate::buffer::Selection;
use crate::settings::Settings;
use crate::tools::is_word_char;

const TEXT_PAD: f32 = 4.0;
const SCROLLBAR: f32 = 12.0;

pub struct EditorElement {
    editor: Entity<Editor>,
}

impl EditorElement {
    pub fn new(editor: Entity<Editor>) -> Self {
        EditorElement { editor }
    }
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

#[allow(dead_code)]
pub struct PaintData {
    bounds: Bounds<Pixels>,
    text_bounds: Bounds<Pixels>,
    gutter: Bounds<Pixels>,
    bg: Option<Hsla>,
    gutter_quads: Vec<PaintQuad>,
    gutter_texts: Vec<(Point<Pixels>, ShapedLine)>,
    under: Vec<PaintQuad>,
    texts: Vec<(Point<Pixels>, ShapedLine)>,
    over: Vec<PaintQuad>,
    over_texts: Vec<(Point<Pixels>, ShapedLine)>,
    carets: Vec<PaintQuad>,
    scrollbars: Vec<PaintQuad>,
    placeholder: Option<(Point<Pixels>, ShapedLine)>,
    line_height: Pixels,
}

pub struct PrepaintState {
    data: PaintData,
    text_hitbox: Hitbox,
    gutter_hitbox: Hitbox,
    whole_hitbox: Hitbox,
}

impl Element for EditorElement {
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
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = gpui::relative(1.).into();
        let editor = self.editor.read(cx);
        if editor.mode == EditorMode::SingleLine {
            let lh = editor.style(cx).line_height;
            style.size.height = lh.into();
        } else {
            style.size.height = gpui::relative(1.).into();
        }
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> PrepaintState {
        let data = self.editor.update(cx, |editor, cx| editor.layout_frame(bounds, window, cx));
        let whole_hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let text_hitbox = window.insert_hitbox(data.text_bounds, HitboxBehavior::Normal);
        let gutter_hitbox = window.insert_hitbox(data.gutter, HitboxBehavior::Normal);
        PrepaintState { data, text_hitbox, gutter_hitbox, whole_hitbox }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        state: &mut PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let d = &mut state.data;
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(&focus_handle, ElementInputHandler::new(d.text_bounds, self.editor.clone()), cx);
        window.set_cursor_style(CursorStyle::IBeam, &state.text_hitbox);
        window.set_cursor_style(CursorStyle::Arrow, &state.gutter_hitbox);

        if let Some(bg) = d.bg {
            window.paint_quad(fill(bounds, bg));
        }
        window.with_content_mask(Some(ContentMask { bounds: d.text_bounds }), |window| {
            for q in d.under.drain(..) {
                window.paint_quad(q);
            }
            for (origin, line) in d.texts.iter() {
                let _ = line.paint(*origin, d.line_height, window, cx);
            }
            if let Some((origin, line)) = &d.placeholder {
                let _ = line.paint(*origin, d.line_height, window, cx);
            }
            for q in d.over.drain(..) {
                window.paint_quad(q);
            }
            for (origin, line) in d.over_texts.iter() {
                let _ = line.paint(*origin, d.line_height, window, cx);
            }
            for q in d.carets.drain(..) {
                window.paint_quad(q);
            }
        });
        window.with_content_mask(Some(ContentMask { bounds: d.gutter }), |window| {
            for q in d.gutter_quads.drain(..) {
                window.paint_quad(q);
            }
            for (origin, line) in d.gutter_texts.iter() {
                let _ = line.paint(*origin, d.line_height, window, cx);
            }
        });
        for q in d.scrollbars.drain(..) {
            window.paint_quad(q);
        }

        // Mouse handling.
        let editor = self.editor.clone();
        let hitbox = state.whole_hitbox.clone();
        window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                editor.update(cx, |ed, cx| ed.mouse_down(e, window, cx));
                cx.stop_propagation();
            }
        });
        let editor = self.editor.clone();
        window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble {
                let dragging = editor.read(cx).drag.is_some();
                if dragging {
                    editor.update(cx, |ed, cx| ed.mouse_drag(e.position, window, cx));
                }
            }
        });
        let editor = self.editor.clone();
        window.on_mouse_event(move |_: &MouseUpEvent, phase, _window, cx| {
            if phase == DispatchPhase::Bubble {
                let dragging = editor.read(cx).drag.is_some();
                if dragging {
                    editor.update(cx, |ed, cx| {
                        ed.drag = None;
                        cx.notify();
                    });
                }
            }
        });
        let editor = self.editor.clone();
        let hitbox = state.whole_hitbox.clone();
        window.on_mouse_event(move |e: &ScrollWheelEvent, phase, window, cx| {
            if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
                let handled = editor.update(cx, |ed, cx| ed.scroll_wheel(e, window, cx));
                if handled {
                    cx.stop_propagation();
                }
            }
        });
    }
}

fn digits(n: usize) -> usize {
    let mut d = 1;
    let mut n = n / 10;
    while n > 0 {
        d += 1;
        n /= 10;
    }
    d
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w.max(0.0)), px(h.max(0.0))))
}

impl Editor {
    fn shape_plain(&self, text: &str, color: Hsla, font_size: Pixels, window: &mut Window, cx: &App) -> ShapedLine {
        let style = self.style(cx);
        let runs = [TextRun {
            len: text.len(),
            font: style.font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        window.text_system().shape_line(SharedString::from(text.to_string()), font_size, &runs, None)
    }

    /// Compute everything needed to paint this frame. Also stores a [`LayoutSnapshot`].
    pub(crate) fn layout_frame(&mut self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut gpui::Context<Self>) -> PaintData {
        let settings = cx.global::<Settings>().clone();
        let theme = crate::theme::theme(cx);
        let style = self.style(cx);
        let full = self.mode == EditorMode::Full;
        let single = self.mode == EditorMode::SingleLine;
        let lh = f32::from(style.line_height);
        let text_system = window.text_system().clone();
        let font_id = text_system.resolve_font(&style.font);
        let font_size = style.font_size;
        let space_w = text_system.advance(font_id, font_size, ' ').map(|s| f32::from(s.width)).unwrap_or(7.0).max(1.0);
        let digit_w = text_system.advance(font_id, font_size, '0').map(|s| f32::from(s.width)).unwrap_or(space_w);
        let key = (style.font.family.clone(), f32::from(font_size));
        if key != self.font_key {
            self.font_key = key;
            self.display.widths = Default::default();
            self.display.invalidate();
        }
        let tab = self.tab_size(cx);
        if self.display.tab_size != tab {
            self.display.tab_size = tab;
            self.display.invalidate();
        }

        // ---- geometry
        let (n_lines, has_hidden) = {
            let b = self.buf(cx);
            (b.len_lines(), !b.hidden_ranges.is_empty())
        };
        let ln_w = if full && settings.show_line_numbers { digits(n_lines.max(99)) as f32 * digit_w + 14.0 } else { 0.0 };
        let bm_w = if full && (settings.show_bookmark_margin || has_hidden) { 16.0 } else { 0.0 };
        let fold_w = if full && settings.show_fold_margin { 14.0 } else { 0.0 };
        let gutter_w = ln_w + bm_w + fold_w;
        let wrap = self.word_wrap(cx);
        let show_v = !single;
        let show_h = full && !wrap;
        let bx = f32::from(bounds.origin.x);
        let by = f32::from(bounds.origin.y);
        let bw = f32::from(bounds.size.width);
        let bh = f32::from(bounds.size.height);
        let vs_w = if show_v { SCROLLBAR } else { 0.0 };
        let hs_h = if show_h { SCROLLBAR } else { 0.0 };
        let gutter = rect(bx, by, gutter_w, bh - hs_h);
        let text_bounds = rect(bx + gutter_w, by, bw - gutter_w - vs_w, bh - hs_h);
        let pad = if full { TEXT_PAD } else { 1.0 };
        let text_left = bx + gutter_w + pad;
        let tw = (bw - gutter_w - vs_w - pad).max(10.0);
        let th = (bh - hs_h).max(lh);

        // ---- wrapping
        if wrap {
            let w = (tw - space_w * 1.5).max(space_w * 8.0).floor();
            if self.display.wrap_width != Some(w) {
                self.display.wrap_width = Some(w);
                self.display.invalidate();
            }
        } else if self.display.wrap_width.is_some() {
            self.display.wrap_width = None;
            self.display.invalidate();
        }
        {
            let mut measure = |c: char| text_system.advance(font_id, font_size, c).map(|s| f32::from(s.width)).unwrap_or(space_w);
            let b = self.buffer.read(cx);
            self.display.ensure(b, &self.folds, &mut measure);
        }
        let row_count = self.display.row_count(self.buf(cx)).max(1);

        // ---- scrolling
        let content_h = row_count as f32 * lh;
        let max_scroll_y = if single {
            0.0
        } else if full && settings.scroll_beyond_last_line {
            ((row_count as f32 - 1.0) * lh).max(0.0)
        } else {
            (content_h - th).max(0.0)
        };
        if let Some(a) = self.autoscroll.take() {
            let caret = self.caret();
            let line = self.buf(cx).char_to_line(caret);
            let shape = self.shape_line(line, window, cx);
            let (seg, x) = shape.x_for_char(caret - shape.start_char);
            let row = self.display.line_to_row(line) + seg;
            let y = row as f32 * lh;
            if !single {
                match a {
                    Autoscroll::Fit => {
                        if y < self.scroll_y {
                            self.scroll_y = y;
                        } else if y + lh > self.scroll_y + th {
                            self.scroll_y = y + lh - th;
                        }
                    }
                    Autoscroll::Center => {
                        if y < self.scroll_y + lh || y + 2.0 * lh > self.scroll_y + th {
                            self.scroll_y = (y - th / 2.0 + lh / 2.0).max(0.0);
                        }
                    }
                    Autoscroll::Top => self.scroll_y = y,
                }
            }
            if !wrap {
                let margin = (space_w * 4.0).min(tw / 4.0);
                if x < self.scroll_x + margin * 0.5 {
                    self.scroll_x = (x - margin * 3.0).max(0.0);
                    if x < tw * 0.5 {
                        self.scroll_x = 0.0;
                    }
                } else if x > self.scroll_x + tw - margin {
                    self.scroll_x = x - tw + tw / 3.0;
                }
            }
            if self.scroll_y != self.last_scroll_emitted.0 || self.scroll_x != self.last_scroll_emitted.1 {
                cx.emit(EditorEvent::Scrolled);
            }
        }
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll_y).round();
        if wrap {
            self.scroll_x = 0.0;
        }
        self.last_scroll_emitted = (self.scroll_y, self.scroll_x);

        // ---- visible rows
        let first_row = (self.scroll_y / lh).floor() as usize;
        let visible = (th / lh).ceil() as usize + 1;
        let last_row = (first_row + visible).min(row_count);
        let mut row_lines: Vec<(usize, usize, usize)> = Vec::with_capacity(visible); // (row, line, sub)
        for row in first_row..last_row {
            let (line, sub) = self.display.row_to_line(row);
            if line >= n_lines {
                break;
            }
            row_lines.push((row, line, sub));
        }
        if let (Some(first), Some(last)) = (row_lines.first(), row_lines.last()) {
            let range = first.1..last.1 + 1;
            let deadline = Instant::now() + Duration::from_millis(8);
            let syn_theme = theme.syntect.clone();
            let done = self.buffer.update(cx, |b, _| b.ensure_highlight(range, &syn_theme, deadline));
            if !done {
                let entity = cx.entity();
                window.on_next_frame(move |_, cx| entity.update(cx, |_, cx| cx.notify()));
            }
        }

        let y_offset = if single { ((f32::from(bounds.size.height) - lh) / 2.0).max(0.0) } else { 0.0 };
        let row_y = |row: usize, scroll_y: f32| by + y_offset + row as f32 * lh - scroll_y;
        let mut rows: Vec<RowInfo> = Vec::with_capacity(row_lines.len());
        let mut shapes: Vec<Rc<super::LineShape>> = Vec::new();
        for &(row, line, sub) in &row_lines {
            let shape = match shapes.last() {
                Some(s) if s.line == line => s.clone(),
                _ => {
                    let s = Rc::new(self.shape_line(line, window, cx));
                    shapes.push(s.clone());
                    s
                }
            };
            rows.push(RowInfo { row, line, sub, y: row_y(row, self.scroll_y), shape });
        }
        for s in &shapes {
            for seg in &s.segs {
                let w = f32::from(seg.shaped.width);
                if w > self.max_content_width {
                    self.max_content_width = w;
                }
            }
        }

        // ---- painting lists
        let mut under: Vec<PaintQuad> = Vec::new();
        let mut over: Vec<PaintQuad> = Vec::new();
        let mut texts = Vec::new();
        let mut over_texts = Vec::new();
        let mut carets = Vec::new();
        let x0 = text_left - self.scroll_x;
        let tb_left = f32::from(text_bounds.origin.x);
        let tb_w = f32::from(text_bounds.size.width);
        let focused = self.focus_handle.is_focused(window);
        let primary = self.primary_selection();
        let buffer = self.buffer.read(cx);
        let caret_line = buffer.char_to_line(primary.head);

        let seg_x = |row: &RowInfo, col: usize| -> f32 {
            let seg = &row.shape.segs[row.sub];
            let disp = row.shape.dl.char_to_disp[col.min(row.shape.dl.char_count())] as usize;
            let local = disp.saturating_sub(seg.disp.start).min(seg.disp.end - seg.disp.start);
            f32::from(seg.shaped.x_for_index(local))
        };
        let row_char_range = |row: &RowInfo| -> (usize, usize) {
            let seg = &row.shape.segs[row.sub];
            (row.shape.start_char + seg.chars.start, row.shape.start_char + seg.chars.end)
        };
        // Highlight a doc char range on all visible rows.
        let range_rects = |start: usize, end: usize, color: Hsla, out: &mut Vec<PaintQuad>, extend_eol: bool| {
            for row in &rows {
                let (rs, re) = row_char_range(row);
                let last_seg = row.sub + 1 == row.shape.segs.len();
                if end < rs || start > re || (start == re && !last_seg) {
                    continue;
                }
                let s = start.max(rs) - row.shape.start_char;
                let e = end.min(re) - row.shape.start_char;
                let xs = seg_x(row, s);
                let mut xe = seg_x(row, e);
                if extend_eol && last_seg && end > re {
                    xe += space_w;
                }
                if xe > xs {
                    out.push(fill(rect(x0 + xs, row.y, xe - xs, lh), color));
                }
            }
        };

        if full && settings.highlight_current_line && !self.has_selection_spanning_lines(buffer) {
            for row in rows.iter().filter(|r| r.line == caret_line) {
                if settings.current_line_frame {
                    under.push(gpui::outline(rect(tb_left, row.y, tb_w, lh), theme.current_line_bg, gpui::BorderStyle::Solid));
                } else {
                    under.push(fill(rect(tb_left, row.y, tb_w, lh), theme.current_line_bg));
                }
            }
        }

        // Indent guides.
        if full && settings.show_indent_guides {
            let tabw = space_w * tab as f32;
            let mut prev_indent = 0usize;
            for row in rows.iter().filter(|r| r.sub == 0) {
                let text = &row.shape.dl.text;
                let ind = match display::line_indent(text, tab) {
                    Some(i) => i,
                    None => {
                        // blank line: use the next non-blank line's indent (bounded lookahead)
                        let mut ind = prev_indent;
                        for l in row.line + 1..(row.line + 40).min(n_lines) {
                            if let Some(i) = display::line_indent(&buffer.line_text(l), tab) {
                                ind = i.min(prev_indent.max(i));
                                break;
                            }
                        }
                        ind
                    }
                };
                prev_indent = ind;
                let levels = ind / tab;
                for k in 1..=levels {
                    let gx = x0 + (k - 1) as f32 * tabw;
                    if k as f32 * tabw > ind as f32 * space_w {
                        break;
                    }
                    if k == 1 && ind == 0 {
                        continue;
                    }
                    if (k - 1) * tab == 0 {
                        continue;
                    }
                    let mut y = row.y;
                    while y < row.y + lh {
                        under.push(fill(rect(gx.round(), y, 1.0, 1.0), theme.indent_guide));
                        y += 2.0;
                    }
                }
            }
        }

        // Edge column.
        if full && settings.show_edge && settings.edge_column > 0 {
            let ex = x0 + settings.edge_column as f32 * space_w;
            under.push(fill(rect(ex.round(), by, 1.0, th), theme.edge));
        }

        // Marks (find marks + token styles).
        if full && !rows.is_empty() {
            let vis_start = rows.first().map(|r| r.shape.start_char).unwrap_or(0);
            let vis_end = rows.last().map(|r| r.shape.start_char + r.shape.len_chars).unwrap_or(0);
            for m in buffer.marks.iter() {
                if m.range.end < vis_start || m.range.start > vis_end {
                    continue;
                }
                if m.style >= 6 {
                    // Compare results: whole-line backgrounds.
                    let color = if m.style == 6 { theme.diff_added } else { theme.diff_removed };
                    let l0 = buffer.char_to_line(m.range.start);
                    let l1 = buffer.char_to_line(m.range.end.max(m.range.start));
                    for row in rows.iter().filter(|r| r.line >= l0 && r.line <= l1) {
                        under.push(fill(rect(tb_left, row.y, tb_w, lh), color));
                    }
                    continue;
                }
                let color = if m.style == 0 { theme.find_mark } else { theme.mark_styles[(m.style as usize - 1).min(4)] };
                range_rects(m.range.start, m.range.end, color, &mut under, false);
            }
            for r in &self.incremental_highlight {
                if r.end < vis_start || r.start > vis_end {
                    continue;
                }
                range_rects(r.start, r.end, theme.incremental_hl, &mut under, false);
            }
            // Smart highlighting.
            if settings.smart_highlight && self.selections.len() == 1 && !primary.is_empty() {
                let needle = buffer.slice_string(primary.range());
                let single_line = !needle.contains(['\n', '\r']);
                let is_word = needle.chars().all(is_word_char);
                if single_line && !needle.trim().is_empty() && needle.chars().count() < 200 && (is_word || !settings.smart_highlight_whole_word) {
                    let case = settings.smart_highlight_match_case;
                    let needle_cmp: Vec<char> = if case { needle.chars().collect() } else { needle.to_lowercase().chars().collect() };
                    let nlen = needle_cmp.len();
                    let mut done_lines = std::collections::HashSet::new();
                    for row in &rows {
                        if !done_lines.insert(row.line) {
                            continue;
                        }
                        let lt = buffer.line_text(row.line);
                        let chars: Vec<char> = if case { lt.chars().collect() } else { lt.to_lowercase().chars().collect() };
                        if chars.len() < nlen {
                            continue;
                        }
                        let mut i = 0;
                        while i + nlen <= chars.len() {
                            if chars[i..i + nlen] == needle_cmp[..] {
                                let ok = !settings.smart_highlight_whole_word
                                    || ((i == 0 || !is_word_char(chars[i - 1])) && (i + nlen == chars.len() || !is_word_char(chars[i + nlen])));
                                if ok {
                                    let s = row.shape.start_char + i;
                                    range_rects(s, s + nlen, theme.smart_hl, &mut under, false);
                                    i += nlen;
                                    continue;
                                }
                            }
                            i += 1;
                        }
                    }
                }
            }
            // Matching HTML/XML tags.
            if settings.match_tag_highlight && primary.is_empty() {
                let syn = &buffer.syntax_name;
                if syn.contains("HTML") || syn.contains("XML") || syn == "PHP" || syn.contains("Vue") || syn == "Svelte" || syn.contains("JSP") {
                    let key = (buffer.version(), primary.head);
                    if self.tag_cache.as_ref().map(|c| c.0) != Some(key) {
                        let m = super::movement::tag_match(buffer, primary.head);
                        self.tag_cache = Some((key, m));
                    }
                    if let Some((_, Some((own, other)))) = self.tag_cache.clone() {
                        range_rects(own.start, own.end, theme.tag_match, &mut under, false);
                        if let Some(o) = other {
                            range_rects(o.start, o.end, theme.tag_match, &mut under, false);
                        }
                    }
                }
            }
            // Brace matching.
            if settings.brace_highlight && primary.is_empty() {
                if let Some((bp, m)) = brace_match(buffer, primary.head) {
                    match m {
                        Some(mp) => {
                            range_rects(bp, bp + 1, theme.brace_match_bg, &mut under, false);
                            range_rects(mp, mp + 1, theme.brace_match_bg, &mut under, false);
                        }
                        None => {
                            let mut c = theme.brace_bad_fg;
                            c.a = 0.35;
                            range_rects(bp, bp + 1, c, &mut under, false);
                        }
                    }
                }
            }
        }

        // Selections.
        let sel_color = if focused || !full { theme.selection_bg } else { theme.selection_inactive_bg };
        let sel_color = if !full && !focused { theme.selection_inactive_bg } else { sel_color };
        for s in &self.selections {
            if !s.is_empty() {
                range_rects(s.start(), s.end(), sel_color, &mut under, full);
            }
        }

        // Control-character boxes and whitespace markers, text.
        let show_ws = full && settings.show_whitespace;
        for row in &rows {
            let seg = &row.shape.segs[row.sub];
            for (r, kind) in row.shape.dl.specials.iter() {
                if r.end <= seg.disp.start || r.start >= seg.disp.end {
                    continue;
                }
                let ls = r.start.max(seg.disp.start) - seg.disp.start;
                let le = r.end.min(seg.disp.end) - seg.disp.start;
                let xs = x0 + f32::from(seg.shaped.x_for_index(ls));
                let xe = x0 + f32::from(seg.shaped.x_for_index(le));
                match kind {
                    SpecialKind::Control => {
                        under.push(gpui::quad(
                            rect(xs, row.y + 1.0, xe - xs, lh - 2.0),
                            Corners::all(px(2.0)),
                            theme.ctrl_bg,
                            gpui::Edges::default(),
                            gpui::transparent_black(),
                            gpui::BorderStyle::Solid,
                        ));
                    }
                    SpecialKind::Space if show_ws => {
                        let cx_ = (xs + xe) / 2.0;
                        over.push(fill(rect(cx_ - 1.0, row.y + lh / 2.0 - 1.0, 2.0, 2.0), theme.whitespace));
                    }
                    SpecialKind::Tab if show_ws => {
                        let ym = (row.y + lh / 2.0).round();
                        let x_end = xe - 3.0;
                        over.push(fill(rect(xs + 2.0, ym, (x_end - xs - 2.0).max(2.0), 1.0), theme.whitespace));
                        over.push(fill(rect(x_end - 3.0, ym - 3.0, 1.0, 7.0), theme.whitespace));
                        over.push(fill(rect(x_end - 2.0, ym - 2.0, 1.0, 5.0), theme.whitespace));
                        over.push(fill(rect(x_end - 1.0, ym - 1.0, 1.0, 3.0), theme.whitespace));
                    }
                    _ => {}
                }
            }
            texts.push((point(px(x0), px(row.y)), seg.shaped.clone()));
        }

        // EOL markers and wrap symbols.
        if full && (settings.show_eol || settings.show_wrap_symbol) {
            let small = px((f32::from(font_size) * 0.72).round());
            for row in &rows {
                let seg = &row.shape.segs[row.sub];
                let last = row.sub + 1 == row.shape.segs.len();
                let xe = x0 + f32::from(seg.shaped.width);
                if last && settings.show_eol {
                    let label = match buffer.line_eol(row.line) {
                        "\r\n" => "CRLF",
                        "\n" => "LF",
                        "\r" => "CR",
                        _ => continue,
                    };
                    let shaped = self.shape_plain(label, theme.bg, small, window, cx);
                    let w = f32::from(shaped.width) + 4.0;
                    under.push(gpui::quad(
                        rect(xe + 2.0, row.y + 2.0, w, lh - 4.0),
                        Corners::all(px(2.0)),
                        theme.whitespace,
                        gpui::Edges::default(),
                        gpui::transparent_black(),
                        gpui::BorderStyle::Solid,
                    ));
                    over_texts.push((point(px(xe + 4.0), px(row.y)), shaped));
                } else if !last && settings.show_wrap_symbol {
                    let shaped = self.shape_plain("↩", theme.whitespace, font_size, window, cx);
                    over_texts.push((point(px(xe + 2.0), px(row.y)), shaped));
                }
            }
        }

        // Carets.
        let caret_w = settings.caret_width.max(1.0);
        if focused && self.cursor_visible {
            for s in &self.selections {
                let pos = s.head;
                for row in &rows {
                    let start = row.shape.start_char;
                    if pos < start || pos > start + row.shape.len_chars {
                        continue;
                    }
                    let col = pos - start;
                    if row.shape.seg_for_char(col) != row.sub {
                        continue;
                    }
                    let x = x0 + seg_x(row, col);
                    if self.overwrite && full {
                        let next_x = if col < row.shape.len_chars { x0 + seg_x(row, col + 1) } else { x + space_w };
                        carets.push(fill(rect(x, row.y + lh - 2.0, (next_x - x).max(space_w * 0.6), 2.0), theme.caret));
                    } else {
                        carets.push(fill(rect(x.round() - 0.5 * (caret_w - 1.0), row.y + 1.0, caret_w, lh - 2.0), if full { theme.caret } else { theme.panel_fg }));
                    }
                }
            }
        }
        // IME marked text underline.
        if let Some(mr) = self.marked_range.clone() {
            for row in &rows {
                let (rs, re) = row_char_range(row);
                if mr.end < rs || mr.start > re {
                    continue;
                }
                let s = mr.start.max(rs) - row.shape.start_char;
                let e = mr.end.min(re) - row.shape.start_char;
                let xs = seg_x(row, s);
                let xe = seg_x(row, e);
                over.push(fill(rect(x0 + xs, row.y + lh - 2.0, xe - xs, 1.5), theme.fg));
            }
        }

        // Placeholder for empty inputs.
        let placeholder = if !full && buffer.len_chars() == 0 && !self.placeholder.is_empty() {
            let shaped = self.shape_plain(&self.placeholder.clone(), theme.muted_fg, font_size, window, cx);
            Some((point(px(x0), px(by + y_offset)), shaped))
        } else {
            None
        };

        // ---- gutter
        let mut gutter_quads = Vec::new();
        let mut gutter_texts = Vec::new();
        if full && gutter_w > 0.0 {
            let gx = bx;
            if ln_w > 0.0 {
                gutter_quads.push(fill(rect(gx, by, ln_w, th), theme.gutter_bg));
            }
            if bm_w + fold_w > 0.0 {
                gutter_quads.push(fill(rect(gx + ln_w, by, bm_w + fold_w, th), theme.margin_bg));
            }
            let hidden_after: std::collections::HashSet<usize> = buffer
                .hidden_ranges
                .iter()
                .map(|r| r.start.saturating_sub(1))
                .collect();
            for row in rows.iter().filter(|r| r.sub == 0) {
                if ln_w > 0.0 {
                    let color = if row.line == caret_line { theme.gutter_active_fg } else { theme.gutter_fg };
                    let label = (row.line + 1).to_string();
                    let shaped = self.shape_plain(&label, color, font_size, window, cx);
                    let x = gx + ln_w - 6.0 - f32::from(shaped.width);
                    gutter_texts.push((point(px(x), px(row.y)), shaped));
                }
                if bm_w > 0.0 {
                    let mx = gx + ln_w;
                    if buffer.bookmarks.contains(&row.line) {
                        let d = (lh - 6.0).min(12.0);
                        gutter_quads.push(gpui::quad(
                            rect(mx + (bm_w - d) / 2.0, row.y + (lh - d) / 2.0, d, d),
                            Corners::all(px(d / 2.0)),
                            theme.bookmark,
                            gpui::Edges::all(px(1.0)),
                            crate::theme::rgb_hex(0x1E4E9A),
                            gpui::BorderStyle::Solid,
                        ));
                    }
                    if hidden_after.contains(&row.line) {
                        gutter_quads.push(fill(rect(mx + 3.0, row.y + lh - 4.0, bm_w - 6.0, 3.0), theme.hidden_marker));
                    }
                }
                if settings.show_change_history {
                    if let Some(saved) = buffer.change_history.get(&row.line) {
                        let color = if *saved { crate::theme::rgb_hex(0x2FA24A) } else { crate::theme::rgb_hex(0xF28C28) };
                        let x = gx + ln_w + bm_w - 4.0;
                        let rows_for_line = row.shape.segs.len() as f32;
                        gutter_quads.push(fill(rect(x.max(gx), row.y, 3.0, lh * rows_for_line), color));
                    }
                }
                if fold_w > 0.0 {
                    let fx = gx + ln_w + bm_w;
                    let folded = self.folds.contains_key(&row.line);
                    if folded || display::is_fold_header(buffer, row.line, tab) {
                        let s = 9.0f32;
                        let bxx = (fx + (fold_w - s) / 2.0).round();
                        let byy = (row.y + (lh - s) / 2.0).round();
                        gutter_quads.push(gpui::quad(
                            rect(bxx, byy, s, s),
                            Corners::all(px(1.0)),
                            theme.margin_bg,
                            gpui::Edges::all(px(1.0)),
                            theme.fold_marker,
                            gpui::BorderStyle::Solid,
                        ));
                        gutter_quads.push(fill(rect(bxx + 2.0, byy + 4.0, s - 4.0, 1.0), theme.fold_marker));
                        if folded {
                            gutter_quads.push(fill(rect(bxx + 4.0, byy + 2.0, 1.0, s - 4.0), theme.fold_marker));
                            // underline the folded header across the text area
                            under.push(fill(rect(tb_left, row.y + lh - 1.0, tb_w, 1.0), theme.fold_marker));
                        }
                    }
                }
            }
        }

        // ---- scrollbars
        let mut scrollbars = Vec::new();
        let mut vscroll = None;
        let mut hscroll = None;
        if show_v {
            let track = rect(bx + bw - vs_w, by, vs_w, bh - hs_h);
            let total = content_h + if full && settings.scroll_beyond_last_line { th - lh } else { 0.0 };
            let visible_frac = (th / total.max(1.0)).min(1.0);
            let track_h = f32::from(track.size.height);
            let thumb_h = (track_h * visible_frac).max(24.0).min(track_h);
            let frac = if max_scroll_y > 0.0 { self.scroll_y / max_scroll_y } else { 0.0 };
            let thumb_y = by + (track_h - thumb_h) * frac;
            let thumb = rect(bx + bw - vs_w + 2.0, thumb_y, vs_w - 4.0, thumb_h);
            scrollbars.push(fill(track, theme.scrollbar_track));
            if visible_frac < 1.0 {
                scrollbars.push(gpui::quad(thumb, Corners::all(px(4.0)), theme.scrollbar_thumb, gpui::Edges::default(), gpui::transparent_black(), gpui::BorderStyle::Solid));
            }
            // bookmark ticks
            if full && n_lines > 0 {
                for l in buffer.bookmarks.iter().take(2000) {
                    let ty = by + track_h * (self.display.line_to_row(*l) as f32 / row_count as f32);
                    scrollbars.push(fill(rect(bx + bw - vs_w + 1.0, ty, vs_w - 2.0, 2.0), theme.bookmark));
                }
            }
            vscroll = Some(ScrollbarInfo { track, thumb, max_scroll: max_scroll_y });
        }
        if show_h {
            let content_w = self.max_content_width + space_w * 4.0;
            let max_scroll_x = (content_w - tw).max(0.0);
            self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x.max(self.scroll_x.min(content_w)));
            let track = rect(bx + gutter_w, by + bh - hs_h, bw - gutter_w - vs_w, hs_h);
            let track_w = f32::from(track.size.width);
            let visible_frac = (tw / content_w.max(1.0)).min(1.0);
            let thumb_w = (track_w * visible_frac).max(24.0).min(track_w);
            let frac = if max_scroll_x > 0.0 { (self.scroll_x / max_scroll_x).min(1.0) } else { 0.0 };
            let thumb = rect(bx + gutter_w + (track_w - thumb_w) * frac, by + bh - hs_h + 2.0, thumb_w, hs_h - 4.0);
            scrollbars.push(fill(track, theme.scrollbar_track));
            if visible_frac < 1.0 {
                scrollbars.push(gpui::quad(thumb, Corners::all(px(4.0)), theme.scrollbar_thumb, gpui::Edges::default(), gpui::transparent_black(), gpui::BorderStyle::Solid));
            }
            hscroll = Some(ScrollbarInfo { track, thumb, max_scroll: max_scroll_x });
            if gutter_w > 0.0 {
                scrollbars.push(fill(rect(bx, by + bh - hs_h, gutter_w, hs_h), theme.gutter_bg));
            }
        }

        self.layout = Some(LayoutSnapshot {
            bounds,
            text_bounds,
            gutter,
            line_numbers_x: bx..bx + ln_w,
            bookmark_x: bx + ln_w..bx + ln_w + bm_w,
            fold_x: bx + ln_w + bm_w..bx + gutter_w,
            line_height: lh,
            space_width: space_w,
            first_row,
            rows,
            row_count,
            vscroll,
            hscroll,
            content_width: self.max_content_width,
        });
        self.text_left = text_left;
        self.y_offset = y_offset;

        PaintData {
            bounds,
            text_bounds,
            gutter,
            bg: if full { Some(theme.bg) } else { None },
            gutter_quads,
            gutter_texts,
            under,
            texts,
            over,
            over_texts,
            carets,
            scrollbars,
            placeholder,
            line_height: style.line_height,
        }
    }

    fn has_selection_spanning_lines(&self, b: &crate::buffer::Buffer) -> bool {
        self.selections.iter().any(|s| !s.is_empty() && b.char_to_line(s.start()) != b.char_to_line(s.end()))
    }

    // ------------------------------------------------------------------ hit testing

    /// Display row and buffer position under a window point.
    pub(crate) fn hit_test(&mut self, pos: Point<Pixels>, window: &mut Window, cx: &mut gpui::Context<Self>) -> (usize, usize, f32) {
        let Some(layout) = self.layout.clone() else { return (0, 0, 0.0) };
        let lh = layout.line_height;
        let rel_y = f32::from(pos.y) - f32::from(layout.bounds.origin.y) - self.y_offset + self.scroll_y;
        let row = if self.mode == EditorMode::SingleLine { 0 } else { (rel_y / lh).floor().max(0.0) as usize };
        let x = f32::from(pos.x) - self.text_left + self.scroll_x;
        if row >= layout.row_count {
            let len = self.buf(cx).len_chars();
            let line = self.buf(cx).char_to_line(len);
            return (len, line, x);
        }
        let (line, sub) = self.display.row_to_line(row);
        let shape = match layout.rows.iter().find(|r| r.row == row) {
            Some(r) => r.shape.clone(),
            None => Rc::new(self.shape_line(line, window, cx)),
        };
        let col = shape.char_for_x(sub, x);
        (shape.start_char + col, line, x.max(0.0))
    }

    pub(crate) fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(layout) = self.layout.clone() else { return };
        window.focus(&self.focus_handle);
        self.completion = None;
        let pos = e.position;
        if e.button == MouseButton::Right {
            let (offset, _, _) = self.hit_test(pos, window, cx);
            if !self.selections.iter().any(|s| s.start() <= offset && offset <= s.end() && !s.is_empty()) {
                self.rect = None;
                self.set_selections(vec![Selection::caret(offset)], cx);
            }
            cx.emit(EditorEvent::ContextMenu(pos));
            return;
        }
        if e.button != MouseButton::Left {
            return;
        }
        // Scrollbars.
        if let Some(v) = layout.vscroll {
            if v.track.contains(&pos) {
                if v.thumb.contains(&pos) {
                    let grab = f32::from(pos.y - v.thumb.origin.y);
                    self.drag = Some(DragState { mode: DragMode::VScrollbar { grab }, origin: Selection::default(), last_pos: pos, sel_index: 0 });
                } else {
                    let page = f32::from(layout.text_bounds.size.height) - layout.line_height;
                    if pos.y < v.thumb.origin.y {
                        self.scroll_y = (self.scroll_y - page).max(0.0);
                    } else {
                        self.scroll_y += page;
                    }
                    cx.emit(EditorEvent::Scrolled);
                }
                cx.notify();
                return;
            }
        }
        if let Some(h) = layout.hscroll {
            if h.track.contains(&pos) {
                if h.thumb.contains(&pos) {
                    let grab = f32::from(pos.x - h.thumb.origin.x);
                    self.drag = Some(DragState { mode: DragMode::HScrollbar { grab }, origin: Selection::default(), last_pos: pos, sel_index: 0 });
                } else {
                    let page = f32::from(layout.text_bounds.size.width) * 0.8;
                    if pos.x < h.thumb.origin.x {
                        self.scroll_x = (self.scroll_x - page).max(0.0);
                    } else {
                        self.scroll_x += page;
                    }
                    cx.emit(EditorEvent::Scrolled);
                }
                cx.notify();
                return;
            }
        }
        // Gutter.
        if self.mode == EditorMode::Full && layout.gutter.contains(&pos) {
            let x = f32::from(pos.x);
            let (_, line, _) = self.hit_test(point(layout.text_bounds.origin.x, pos.y), window, cx);
            if layout.fold_x.contains(&x) {
                let tab = self.tab_size(cx);
                if self.folds.contains_key(&line) || display::is_fold_header(self.buf(cx), line, tab) {
                    self.toggle_fold_line(line, cx);
                }
                return;
            }
            if layout.bookmark_x.contains(&x) {
                let hidden_after = self.buf(cx).hidden_ranges.iter().any(|r| r.start == line + 1 || (r.start == 0 && line == 0));
                if hidden_after {
                    self.unhide_range_after(line, cx);
                } else {
                    self.toggle_bookmark_at_line(line, cx);
                }
                return;
            }
            // Line number margin: select lines.
            let b = self.buf(cx);
            let start = b.line_to_char(line);
            let end = if line + 1 < b.len_lines() { b.line_to_char(line + 1) } else { b.len_chars() };
            let sel = if e.modifiers.shift {
                let p = self.primary_selection();
                if start < p.anchor { Selection::new(p.anchor.max(p.end()), start) } else { Selection::new(p.start(), end) }
            } else {
                Selection::new(start, end)
            };
            self.rect = None;
            self.set_selections(vec![sel], cx);
            self.drag = Some(DragState { mode: DragMode::Line, origin: Selection::new(start, end), last_pos: pos, sel_index: 0 });
            return;
        }

        let (offset, line, x) = self.hit_test(pos, window, cx);
        let multi = cx.global::<Settings>().multi_editing && self.mode == EditorMode::Full;
        match e.click_count {
            1 => {
                if e.modifiers.alt && self.mode == EditorMode::Full {
                    let space = layout.space_width;
                    let snapped = (x / space).round() * space;
                    self.rect = Some(RectSelection { anchor_line: line, anchor_x: snapped, head_line: line, head_x: snapped });
                    self.apply_rect(window, cx);
                    self.drag = Some(DragState { mode: DragMode::Column, origin: Selection::caret(offset), last_pos: pos, sel_index: 0 });
                } else if e.modifiers.platform && multi {
                    self.rect = None;
                    let mut sels = self.selections.clone();
                    if let Some(i) = sels.iter().position(|s| s.is_empty() && s.head == offset) {
                        if sels.len() > 1 {
                            sels.remove(i);
                        }
                        self.set_selections(sels, cx);
                    } else {
                        sels.push(Selection::caret(offset));
                        self.set_selections_with_primary(sels, Some(offset), cx);
                        let idx = self.primary;
                        self.drag = Some(DragState { mode: DragMode::Char, origin: Selection::caret(offset), last_pos: pos, sel_index: idx });
                    }
                } else if e.modifiers.shift {
                    self.rect = None;
                    let p = self.primary_selection();
                    self.set_selections(vec![Selection::new(p.anchor, offset)], cx);
                    self.drag = Some(DragState { mode: DragMode::Char, origin: Selection::caret(p.anchor), last_pos: pos, sel_index: 0 });
                } else {
                    self.rect = None;
                    self.buffer.update(cx, |b, _| b.break_undo_group());
                    self.set_selections(vec![Selection::caret(offset)], cx);
                    self.drag = Some(DragState { mode: DragMode::Char, origin: Selection::caret(offset), last_pos: pos, sel_index: 0 });
                }
            }
            2 => {
                self.rect = None;
                let r = word_range_at(self.buf(cx), offset);
                self.set_selections(vec![Selection::new(r.start, r.end)], cx);
                self.drag = Some(DragState { mode: DragMode::Word, origin: Selection::new(r.start, r.end), last_pos: pos, sel_index: 0 });
            }
            _ => {
                self.rect = None;
                let b = self.buf(cx);
                let start = b.line_to_char(line);
                let end = if line + 1 < b.len_lines() { b.line_to_char(line + 1) } else { b.len_chars() };
                self.set_selections(vec![Selection::new(start, end)], cx);
                self.drag = Some(DragState { mode: DragMode::Line, origin: Selection::new(start, end), last_pos: pos, sel_index: 0 });
            }
        }
        self.autoscroll = None;
    }

    pub(crate) fn mouse_drag(&mut self, pos: Point<Pixels>, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(mut drag) = self.drag.clone() else { return };
        let Some(layout) = self.layout.clone() else { return };
        drag.last_pos = pos;
        self.drag = Some(drag.clone());
        match drag.mode {
            DragMode::VScrollbar { grab } => {
                if let Some(v) = layout.vscroll {
                    let track_h = f32::from(v.track.size.height) - f32::from(v.thumb.size.height);
                    let y = f32::from(pos.y - v.track.origin.y) - grab;
                    self.scroll_y = if track_h > 0.0 { (y / track_h).clamp(0.0, 1.0) * v.max_scroll } else { 0.0 };
                    cx.emit(EditorEvent::Scrolled);
                    cx.notify();
                }
                return;
            }
            DragMode::HScrollbar { grab } => {
                if let Some(h) = layout.hscroll {
                    let track_w = f32::from(h.track.size.width) - f32::from(h.thumb.size.width);
                    let x = f32::from(pos.x - h.track.origin.x) - grab;
                    self.scroll_x = if track_w > 0.0 { (x / track_w).clamp(0.0, 1.0) * h.max_scroll } else { 0.0 };
                    cx.emit(EditorEvent::Scrolled);
                    cx.notify();
                }
                return;
            }
            _ => {}
        }
        // Auto-scroll when dragging outside the text area.
        let tb = layout.text_bounds;
        let lh = layout.line_height;
        let mut scrolled = false;
        if self.mode != EditorMode::SingleLine {
            if pos.y < tb.origin.y {
                self.scroll_y = (self.scroll_y - lh.max(f32::from(tb.origin.y - pos.y))).max(0.0);
                scrolled = true;
            } else if pos.y > tb.origin.y + tb.size.height {
                self.scroll_y += lh.max(f32::from(pos.y - tb.origin.y - tb.size.height));
                scrolled = true;
            }
        }
        if pos.x < tb.origin.x && self.scroll_x > 0.0 {
            self.scroll_x = (self.scroll_x - layout.space_width * 3.0).max(0.0);
            scrolled = true;
        } else if pos.x > tb.origin.x + tb.size.width && self.display.wrap_width.is_none() {
            self.scroll_x += layout.space_width * 3.0;
            scrolled = true;
        }
        let (offset, line, x) = self.hit_test(pos, window, cx);
        match drag.mode {
            DragMode::Char => {
                let mut sels = self.selections.clone();
                let i = drag.sel_index.min(sels.len().saturating_sub(1));
                if let Some(s) = sels.get_mut(i) {
                    *s = Selection::new(drag.origin.anchor, offset);
                }
                let head = offset;
                self.set_selections_with_primary(sels, Some(head), cx);
            }
            DragMode::Word => {
                let r = word_range_at(self.buf(cx), offset);
                let o = drag.origin;
                let sel = if offset < o.start() { Selection::new(o.end(), r.start) } else { Selection::new(o.start(), r.end.max(o.end())) };
                self.set_selections(vec![sel], cx);
            }
            DragMode::Line => {
                let b = self.buf(cx);
                let start = b.line_to_char(line);
                let end = if line + 1 < b.len_lines() { b.line_to_char(line + 1) } else { b.len_chars() };
                let o = drag.origin;
                let sel = if start < o.start() { Selection::new(o.end(), start) } else { Selection::new(o.start(), end.max(o.end())) };
                self.set_selections(vec![sel], cx);
            }
            DragMode::Column => {
                if let Some(mut r) = self.rect {
                    let space = layout.space_width;
                    r.head_line = line;
                    r.head_x = (x / space).round() * space;
                    self.rect = Some(r);
                    self.apply_rect(window, cx);
                }
            }
            _ => {}
        }
        self.autoscroll = None;
        if scrolled {
            cx.emit(EditorEvent::Scrolled);
            // Keep scrolling while the mouse stays outside.
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(40)).await;
                let _ = this.update_in(cx, |ed, window, cx| {
                    if ed.drag.as_ref().map(|d| d.last_pos == pos).unwrap_or(false) {
                        ed.mouse_drag(pos, window, cx);
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    pub(crate) fn scroll_wheel(&mut self, e: &ScrollWheelEvent, _window: &mut Window, cx: &mut gpui::Context<Self>) -> bool {
        if self.mode == EditorMode::SingleLine {
            return false;
        }
        let lh = self.layout.as_ref().map(|l| l.line_height).unwrap_or(18.0);
        let delta = e.delta.pixel_delta(px(lh));
        if e.modifiers.platform && self.mode == EditorMode::Full {
            self.zoom_accum += f32::from(delta.y);
            let step = if e.delta.precise() { 30.0 } else { 1.0 };
            if self.zoom_accum.abs() >= step {
                let dir = if self.zoom_accum > 0.0 { 1 } else { -1 };
                self.zoom_accum = 0.0;
                cx.emit(EditorEvent::Zoom(dir));
            }
            return true;
        }
        let (mut dx, mut dy) = (f32::from(delta.x), f32::from(delta.y));
        if e.modifiers.shift && dx == 0.0 {
            dx = dy;
            dy = 0.0;
        }
        if !e.delta.precise() {
            dy *= 3.0;
        }
        self.scroll_y = (self.scroll_y - dy).max(0.0);
        if self.display.wrap_width.is_none() {
            self.scroll_x = (self.scroll_x - dx).max(0.0);
        }
        self.autoscroll = None;
        cx.emit(EditorEvent::Scrolled);
        cx.notify();
        true
    }
}

