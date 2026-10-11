//! Display layer: how buffer lines become rows on screen.
//!
//! * tabs are expanded to spaces and control characters become mnemonic boxes ("NUL", "SOH", ...)
//! * long lines are soft-wrapped when word wrap is on
//! * folded / hidden line ranges produce zero rows

use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

use unicode_width::UnicodeWidthChar;

use crate::buffer::{Buffer, EditInfo};

pub const CONTROL_NAMES: [&str; 32] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "HT", "LF", "VT", "FF", "CR", "SO", "SI",
    "DLE", "DC1", "DC2", "DC3", "DC4", "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecialKind {
    Tab,
    Control,
    Space,
}

/// A buffer line converted to the text that is actually shaped.
#[derive(Clone, Debug, Default)]
pub struct DisplayLine {
    pub text: String,
    /// For every buffer char index 0..=n, the byte offset in `text` where it starts.
    pub char_to_disp: Vec<u32>,
    /// Display byte ranges with special rendering.
    pub specials: Vec<(Range<usize>, SpecialKind)>,
}

impl DisplayLine {
    pub fn build(line: &str, tab_size: usize, mark_spaces: bool) -> DisplayLine {
        let tab_size = tab_size.max(1);
        let mut text = String::with_capacity(line.len() + 8);
        let mut map = Vec::with_capacity(line.len() + 1);
        let mut specials = Vec::new();
        let mut col = 0usize;
        for c in line.chars() {
            map.push(text.len() as u32);
            let start = text.len();
            match c {
                '\t' => {
                    let n = tab_size - (col % tab_size);
                    for _ in 0..n {
                        text.push(' ');
                    }
                    col += n;
                    specials.push((start..text.len(), SpecialKind::Tab));
                }
                c if (c as u32) < 32 || c == '\u{7f}' => {
                    let name = if c == '\u{7f}' { "DEL" } else { CONTROL_NAMES[c as usize] };
                    text.push_str(name);
                    col += name.len();
                    specials.push((start..text.len(), SpecialKind::Control));
                }
                ' ' => {
                    text.push(' ');
                    col += 1;
                    if mark_spaces {
                        specials.push((start..text.len(), SpecialKind::Space));
                    }
                }
                c => {
                    text.push(c);
                    col += c.width().unwrap_or(1).max(if c.is_control() { 0 } else { 1 });
                }
            }
        }
        map.push(text.len() as u32);
        DisplayLine { text, char_to_disp: map, specials }
    }

    pub fn char_count(&self) -> usize {
        self.char_to_disp.len() - 1
    }

    /// Map a display byte offset to the nearest buffer char index.
    pub fn disp_to_char(&self, byte: usize) -> usize {
        let b = byte as u32;
        match self.char_to_disp.binary_search(&b) {
            Ok(i) => i,
            Err(i) => {
                // `byte` falls inside an expanded char (tab / control box). Pick the closer edge.
                let prev = i.saturating_sub(1);
                let next = i.min(self.char_to_disp.len() - 1);
                let dp = b - self.char_to_disp[prev];
                let dn = self.char_to_disp[next].saturating_sub(b);
                if dn < dp { next } else { prev }
            }
        }
    }
}

/// Cached per-char advance widths for the editor font.
#[derive(Clone, Default)]
#[allow(dead_code)]
pub struct CharWidths {
    pub ascii: Vec<f32>,
    pub other: HashMap<char, f32>,
    pub space: f32,
}

impl CharWidths {
    pub fn width(&mut self, c: char, measure: &mut dyn FnMut(char) -> f32) -> f32 {
        if (c as u32) < 128 {
            let i = c as usize;
            if self.ascii.is_empty() {
                self.ascii = vec![-1.0; 128];
            }
            if self.ascii[i] < 0.0 {
                self.ascii[i] = measure(c);
            }
            self.ascii[i]
        } else {
            *self.other.entry(c).or_insert_with(|| measure(c))
        }
    }
}

/// Compute the display byte offsets at which wrapped rows start (excluding 0).
pub fn wrap_boundaries(text: &str, max_width: f32, widths: &mut CharWidths, measure: &mut dyn FnMut(char) -> f32) -> Vec<usize> {
    let mut out = Vec::new();
    if max_width <= 0.0 {
        return out;
    }
    let mut row_start = 0usize;
    let mut width = 0.0f32;
    let mut last_break: Option<usize> = None; // byte index where a new row may start
    let mut width_at_break = 0.0f32;
    let mut prev_space = false;
    for (i, c) in text.char_indices() {
        let w = widths.width(c, measure);
        let is_space = c == ' ';
        if !is_space && prev_space && i > row_start {
            last_break = Some(i);
            width_at_break = width;
        }
        if width + w > max_width && i > row_start && !is_space {
            let brk = match last_break {
                Some(b) if b > row_start => b,
                _ => i,
            };
            out.push(brk);
            width = if brk == i { 0.0 } else { width - width_at_break };
            row_start = brk;
            last_break = None;
        }
        // CJK and similar can break anywhere.
        if c.width().unwrap_or(1) == 2 {
            last_break = Some(i + c.len_utf8());
            width_at_break = width + w;
        }
        width += w;
        prev_space = is_space;
    }
    out
}

/// Maps buffer lines to display rows, accounting for folding, hidden lines and wrapping.
pub struct DisplayMap {
    pub wrap_width: Option<f32>,
    pub tab_size: usize,
    /// Wrapped row count per line (only maintained when wrapping).
    wrap_rows: Vec<u32>,
    /// prefix[i] = number of display rows before line i. Length = line count + 1.
    prefix: Vec<u32>,
    dirty: bool,
    pub widths: CharWidths,
    version: u64,
    identity: bool,
    /// Lines whose wrap count must be recomputed (inclusive start, exclusive end).
    wrap_dirty: Option<(usize, usize)>,
}

impl DisplayMap {
    pub fn new() -> Self {
        DisplayMap {
            wrap_width: None,
            tab_size: 4,
            wrap_rows: Vec::new(),
            prefix: Vec::new(),
            dirty: true,
            widths: CharWidths::default(),
            version: u64::MAX,
            identity: true,
            wrap_dirty: None,
        }
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
        self.wrap_rows.clear();
        self.wrap_dirty = None;
    }

    pub fn invalidate_folds(&mut self) {
        self.dirty = true;
    }

    /// Keep wrap counts in sync with buffer edits.
    pub fn apply_edits(&mut self, edits: &[EditInfo]) {
        self.dirty = true;
        if self.wrap_rows.is_empty() {
            return;
        }
        for e in edits.iter().rev() {
            let old_range = e.start_line..(e.old_end_line + 1).min(self.wrap_rows.len());
            if old_range.start > self.wrap_rows.len() {
                self.wrap_rows.clear();
                return;
            }
            // u32::MAX marks "needs recomputation".
            let replacement = std::iter::repeat(u32::MAX).take(e.new_line_count + 1);
            self.wrap_rows.splice(old_range, replacement);
        }
        // Final positions of the replaced lines (edits are sorted ascending in original coordinates).
        let mut lo = usize::MAX;
        let mut hi = 0usize;
        let mut shift: isize = 0;
        for e in edits.iter() {
            let start = (e.start_line as isize + shift).max(0) as usize;
            lo = lo.min(start);
            hi = hi.max(start + e.new_line_count + 1);
            shift += e.new_line_count as isize - (e.old_end_line - e.start_line) as isize;
        }
        let hi = hi.min(self.wrap_rows.len());
        self.wrap_dirty = match self.wrap_dirty {
            Some((a, b)) => Some((a.min(lo), b.max(hi).min(self.wrap_rows.len()))),
            None => Some((lo, hi)),
        };
    }

    pub fn ensure(&mut self, buffer: &Buffer, folds: &BTreeMap<usize, usize>, measure: &mut dyn FnMut(char) -> f32) {
        let n = buffer.len_lines();
        let no_hidden = folds.is_empty() && buffer.hidden_ranges.is_empty();
        if self.wrap_width.is_none() && no_hidden {
            self.identity = true;
            self.dirty = false;
            self.version = buffer.version();
            return;
        }
        self.identity = false;
        if !self.dirty && self.version == buffer.version() && self.prefix.len() == n + 1 {
            return;
        }
        if let Some(w) = self.wrap_width {
            let range = if self.wrap_rows.len() != n {
                self.wrap_rows = vec![u32::MAX; n];
                0..n
            } else {
                match self.wrap_dirty.take() {
                    Some((a, b)) => a.min(n)..b.min(n),
                    None => 0..0,
                }
            };
            self.wrap_dirty = None;
            if !range.is_empty() {
                let space_w = self.widths.width(' ', measure);
                let mono = space_w > 0.0
                    && (self.widths.width('W', measure) - space_w).abs() < 0.01
                    && (self.widths.width('i', measure) - space_w).abs() < 0.01;
                // Lines narrower than this can't wrap (one char of slack for float error).
                let safe_cols = if mono { ((w - space_w) / space_w).floor().max(0.0) as usize } else { 0 };
                let tab = self.tab_size.max(1);
                let rope = buffer.rope();
                for (i, slice) in rope.lines_at(range.start).enumerate() {
                    let line = range.start + i;
                    if line >= range.end {
                        break;
                    }
                    let mut cols = 0usize;
                    let mut fast = mono;
                    if fast {
                        'scan: for chunk in slice.chunks() {
                            for &b in chunk.as_bytes() {
                                match b {
                                    b'\t' => cols += tab - cols % tab,
                                    b'\n' | b'\r' => {}
                                    0x20..=0x7e => cols += 1,
                                    _ => {
                                        fast = false;
                                        break 'scan;
                                    }
                                }
                                if cols > safe_cols {
                                    fast = false;
                                    break 'scan;
                                }
                            }
                        }
                    }
                    self.wrap_rows[line] = if fast {
                        1
                    } else {
                        let text = buffer.line_text(line);
                        let dl = DisplayLine::build(&text, self.tab_size, false);
                        wrap_boundaries(&dl.text, w, &mut self.widths, measure).len() as u32 + 1
                    };
                }
            }
            // Anything still unknown (shouldn't happen) counts as one row.
            for r in self.wrap_rows.iter_mut() {
                if *r == u32::MAX {
                    *r = 1;
                }
            }
        } else {
            self.wrap_rows.clear();
        }
        let mut hidden = vec![false; 0];
        if !no_hidden {
            hidden = vec![false; n];
            for (&h, &e) in folds.iter() {
                for l in (h + 1)..=e.min(n.saturating_sub(1)) {
                    hidden[l] = true;
                }
            }
            for r in &buffer.hidden_ranges {
                for l in r.start..r.end.min(n) {
                    hidden[l] = true;
                }
            }
        }
        self.prefix.clear();
        self.prefix.reserve(n + 1);
        let mut acc = 0u32;
        self.prefix.push(0);
        for line in 0..n {
            let rows = if !hidden.is_empty() && hidden[line] {
                0
            } else if self.wrap_width.is_some() {
                self.wrap_rows[line]
            } else {
                1
            };
            acc += rows;
            self.prefix.push(acc);
        }
        self.dirty = false;
        self.version = buffer.version();
    }

    pub fn row_count(&self, buffer: &Buffer) -> usize {
        if self.identity {
            buffer.len_lines()
        } else {
            *self.prefix.last().unwrap_or(&0) as usize
        }
    }

    pub fn line_rows(&self, line: usize) -> usize {
        if self.identity {
            1
        } else if line + 1 < self.prefix.len() {
            (self.prefix[line + 1] - self.prefix[line]) as usize
        } else {
            1
        }
    }

    pub fn is_line_visible(&self, line: usize) -> bool {
        self.line_rows(line) > 0
    }

    /// First display row of a buffer line (for hidden lines: the row of the next visible line).
    pub fn line_to_row(&self, line: usize) -> usize {
        if self.identity {
            line
        } else {
            self.prefix.get(line).copied().unwrap_or_else(|| *self.prefix.last().unwrap_or(&0)) as usize
        }
    }

    /// Buffer line and wrap sub-row for a display row.
    pub fn row_to_line(&self, row: usize) -> (usize, usize) {
        if self.identity {
            return (row, 0);
        }
        if self.prefix.len() < 2 {
            return (0, 0);
        }
        let r = row as u32;
        let idx = self.prefix.partition_point(|&p| p <= r);
        let line = idx.saturating_sub(1).min(self.prefix.len() - 2);
        // Skip hidden lines (zero rows) backwards.
        let mut l = line;
        while l > 0 && self.prefix[l + 1] == self.prefix[l] {
            l -= 1;
        }
        (l, (r.saturating_sub(self.prefix[l])) as usize)
    }

}

// ------------------------------------------------------------------ folding

/// Visual indentation of a line, or None when the line is blank.
pub fn line_indent(text: &str, tab_size: usize) -> Option<usize> {
    let mut col = 0;
    for c in text.chars() {
        match c {
            ' ' => col += 1,
            '\t' => col += tab_size - (col % tab_size),
            _ => return Some(col),
        }
    }
    None
}

/// Visual indentation width of a line (0 for blank lines).
pub fn line_indent_chars(text: &str, tab_size: usize) -> usize {
    line_indent(text, tab_size).unwrap_or(0)
}

/// Number of leading chars whose visual width does not exceed `visual`.
pub fn char_offset_for_indent(text: &str, visual: usize, tab_size: usize) -> usize {
    let mut col = 0;
    let mut n = 0;
    for c in text.chars() {
        let w = match c {
            ' ' => 1,
            '\t' => tab_size - (col % tab_size),
            _ => break,
        };
        if col + w > visual {
            break;
        }
        col += w;
        n += 1;
    }
    n
}

/// Cheap check: does this line start an indentation fold region?
pub fn is_fold_header(buffer: &Buffer, line: usize, tab_size: usize) -> bool {
    let Some(base) = line_indent(&buffer.line_text(line), tab_size) else { return false };
    let n = buffer.len_lines();
    let mut l = line + 1;
    while l < n && l < line + 64 {
        if let Some(ind) = line_indent(&buffer.line_text(l), tab_size) {
            return ind > base;
        }
        l += 1;
    }
    false
}

/// If `line` starts an indentation-based fold region, return the last line of the region.
pub fn fold_region(buffer: &Buffer, line: usize, tab_size: usize, scan_limit: usize) -> Option<usize> {
    let n = buffer.len_lines();
    let text = buffer.line_text(line);
    let base = line_indent(&text, tab_size)?;
    let mut l = line + 1;
    let mut first_body = None;
    while l < n && l < line + 1 + scan_limit {
        match line_indent(&buffer.line_text(l), tab_size) {
            None => l += 1,
            Some(ind) => {
                if ind > base {
                    first_body = Some(l);
                }
                break;
            }
        }
    }
    first_body?;
    let mut last = line;
    let mut l = line + 1;
    let limit = (line + 1 + scan_limit).min(n);
    while l < limit {
        match line_indent(&buffer.line_text(l), tab_size) {
            None => {}
            Some(ind) => {
                if ind <= base {
                    break;
                }
                last = l;
            }
        }
        l += 1;
    }
    if last > line {
        Some(last)
    } else {
        None
    }
}

/// All fold regions in the document with their nesting level (1-based).
pub fn all_fold_regions(buffer: &Buffer, tab_size: usize) -> Vec<(usize, usize, usize)> {
    let n = buffer.len_lines();
    let indents: Vec<Option<usize>> = (0..n).map(|l| line_indent(&buffer.line_text(l), tab_size)).collect();
    // next non-blank indent after each line
    let mut next_indent = vec![None; n];
    let mut nxt = None;
    for l in (0..n).rev() {
        next_indent[l] = nxt;
        if indents[l].is_some() {
            nxt = indents[l];
        }
    }
    let mut out = Vec::new();
    let mut stack: Vec<(usize, usize, usize)> = Vec::new(); // (header, indent, out_index)
    let mut last_nonblank = 0usize;
    for l in 0..n {
        if let Some(ind) = indents[l] {
            while let Some(&(h, hind, oi)) = stack.last() {
                if ind <= hind {
                    let end: usize = last_nonblank;
                    out[oi] = (h, end, stack.len());
                    stack.pop();
                } else {
                    break;
                }
            }
            if let Some(ni) = next_indent[l] {
                if ni > ind {
                    out.push((l, l, 0));
                    stack.push((l, ind, out.len() - 1));
                }
            }
            last_nonblank = l;
        }
    }
    while let Some((h, _, oi)) = stack.pop() {
        let depth = stack.len() + 1;
        out[oi] = (h, last_nonblank, depth);
    }
    out.retain(|(h, e, _)| e > h);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_line_tabs_and_controls() {
        let d = DisplayLine::build("a\tb\0c", 4, false);
        assert_eq!(d.text, "a   bNULc");
        assert_eq!(d.char_to_disp, vec![0, 1, 4, 5, 8, 9]);
        assert_eq!(d.disp_to_char(2), 1);
        assert_eq!(d.disp_to_char(3), 2);
    }

    #[test]
    fn wrapping() {
        let mut w = CharWidths::default();
        let mut m = |_c: char| 1.0f32;
        let b = wrap_boundaries("hello world foo", 8.0, &mut w, &mut m);
        assert_eq!(b, vec![6, 12]);
        let b = wrap_boundaries("abcdefghij", 4.0, &mut w, &mut m);
        assert_eq!(b, vec![4, 8]);
    }

    #[test]
    fn indents() {
        assert_eq!(line_indent("\t  x", 4), Some(6));
        assert_eq!(line_indent("   ", 4), None);
    }
}
