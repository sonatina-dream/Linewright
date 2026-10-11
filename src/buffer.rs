//! The document model: a rope of text plus undo history, file metadata, bookmarks and marks.
//!
//! A `Buffer` is shared by every editor view that shows it (View > Clone to Other View).
//! Positions are char offsets into the rope. Line breaks are kept exactly as they are in
//! the file (CR LF, LF or CR); mixed line endings are preserved.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use gpui::{Context, EventEmitter};
use ropey::Rope;

use crate::encoding::{self, EolMode, TextEncoding};
use crate::syntax::{self, SyntaxHighlighter};

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
    /// Remembered x position (in pixels) for vertical movement.
    pub goal: Option<f32>,
}

impl Selection {
    pub fn caret(pos: usize) -> Self {
        Selection { anchor: pos, head: pos, goal: None }
    }
    pub fn new(anchor: usize, head: usize) -> Self {
        Selection { anchor, head, goal: None }
    }
    pub fn start(&self) -> usize {
        self.anchor.min(self.head)
    }
    pub fn end(&self) -> usize {
        self.anchor.max(self.head)
    }
    pub fn range(&self) -> Range<usize> {
        self.start()..self.end()
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
    pub fn reversed(&self) -> bool {
        self.head < self.anchor
    }
}

/// Description of a single replacement, in the coordinates of the text *before* the edit batch.
#[derive(Clone, Debug)]
pub struct EditInfo {
    pub start: usize,
    pub old_len: usize,
    pub new_len: usize,
    pub start_line: usize,
    pub old_end_line: usize,
    pub new_line_count: usize,
}

/// Map a position through a batch of edits (sorted ascending, original coordinates).
pub fn map_pos(pos: usize, edits: &[EditInfo], bias_right: bool) -> usize {
    let mut delta: isize = 0;
    for e in edits {
        if e.old_len == 0 {
            // Pure insertion.
            if pos < e.start || (pos == e.start && !bias_right) {
                break;
            }
            delta += e.new_len as isize;
            continue;
        }
        let old_end = e.start + e.old_len;
        if pos <= e.start {
            break;
        }
        if pos >= old_end {
            delta += e.new_len as isize - e.old_len as isize;
            continue;
        }
        // Strictly inside the replaced range.
        let base = (e.start as isize + delta).max(0) as usize;
        return if bias_right { base + e.new_len } else { base };
    }
    (pos as isize + delta).max(0) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    Typing,
    Delete,
    Other,
}

#[derive(Clone, Debug)]
struct EditRecord {
    start: usize,
    old: String,
    new: String,
    old_len: usize,
    new_len: usize,
}

#[derive(Clone, Debug)]
struct Transaction {
    id: u64,
    batches: Vec<Vec<EditRecord>>,
    sel_before: Vec<Selection>,
    sel_after: Vec<Selection>,
    kind: EditKind,
    time: Instant,
    mergeable: bool,
}

#[derive(Default)]
struct History {
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    next_id: u64,
}

impl History {
    fn top_id(&self) -> u64 {
        self.undo.last().map(|t| t.id).unwrap_or(0)
    }
}

/// A styled range marker (Search > Mark / Style token). Style 0 is the "Find Mark" style,
/// 1..=5 are the "Using 1st..5th Style" token styles.
#[derive(Clone, Debug)]
pub struct Mark {
    pub range: Range<usize>,
    pub style: u8,
}

#[derive(Clone, Debug)]
pub enum BufferEvent {
    /// `origin` is the view id of the editor that made the edit (0 = programmatic).
    Edited { infos: Arc<Vec<EditInfo>>, origin: u64 },
    /// The whole content was replaced (reload / encoding change); views should clamp.
    Reloaded,
    Saved,
    MetaChanged,
}

pub struct Buffer {
    pub id: u64,
    rope: Rope,
    pub file: Option<PathBuf>,
    pub untitled_name: String,
    pub encoding: TextEncoding,
    pub eol: EolMode,
    pub syntax_name: String,
    pub user_read_only: bool,
    pub file_read_only: bool,
    history: History,
    saved_id: u64,
    meta_dirty: bool,
    pub disk_mtime: Option<SystemTime>,
    pub disk_missing: bool,
    pub highlighter: SyntaxHighlighter,
    pub bookmarks: BTreeSet<usize>,
    pub marks: Vec<Mark>,
    pub monitoring: bool,
    version: u64,
    text_snapshot: Option<(u64, Arc<String>)>,
    pub backup_file: Option<PathBuf>,
    pub backup_version: u64,
    /// Lines hidden with View > Hide Lines (shared across views): header line -> last hidden line.
    pub hidden_ranges: Vec<Range<usize>>,
    /// Change history margin: line -> saved? (false = modified and unsaved).
    pub change_history: BTreeMap<usize, bool>,
}

impl EventEmitter<BufferEvent> for Buffer {}

pub struct LoadedFile {
    pub text: String,
    pub encoding: TextEncoding,
    pub eol: Option<EolMode>,
    pub mtime: Option<SystemTime>,
    pub read_only: bool,
}

pub fn load_file(path: &Path) -> anyhow::Result<LoadedFile> {
    let bytes = std::fs::read(path)?;
    let meta = std::fs::metadata(path).ok();
    let decoded = encoding::detect_and_decode(&bytes);
    let eol = EolMode::detect(&decoded.text);
    Ok(LoadedFile {
        text: decoded.text,
        encoding: decoded.encoding,
        eol,
        mtime: meta.as_ref().and_then(|m| m.modified().ok()),
        read_only: meta.map(|m| m.permissions().readonly()).unwrap_or(false),
    })
}

impl Buffer {
    pub fn new(id: u64, untitled_name: String, eol: EolMode, encoding: TextEncoding) -> Self {
        Buffer {
            id,
            rope: Rope::new(),
            file: None,
            untitled_name,
            encoding,
            eol,
            syntax_name: syntax::PLAIN_TEXT.into(),
            user_read_only: false,
            file_read_only: false,
            history: History { next_id: 1, ..Default::default() },
            saved_id: 0,
            meta_dirty: false,
            disk_mtime: None,
            disk_missing: false,
            highlighter: SyntaxHighlighter::new(None),
            bookmarks: BTreeSet::new(),
            marks: Vec::new(),
            monitoring: false,
            version: 0,
            text_snapshot: None,
            backup_file: None,
            backup_version: 0,
            hidden_ranges: Vec::new(),
            change_history: BTreeMap::new(),
        }
    }

    pub fn from_loaded(id: u64, path: PathBuf, loaded: LoadedFile, default_eol: EolMode) -> Self {
        let mut b = Buffer::new(id, String::new(), loaded.eol.unwrap_or(default_eol), loaded.encoding);
        b.rope = Rope::from_str(&loaded.text);
        b.disk_mtime = loaded.mtime;
        b.file_read_only = loaded.read_only;
        let first_line = b.line_text(0);
        b.set_syntax(syntax::detect_syntax(&path, &first_line, b.rope.len_bytes()));
        b.file = Some(path);
        b
    }

    pub fn set_text_initial(&mut self, text: &str) {
        self.rope = Rope::from_str(text);
        self.version += 1;
        self.highlighter.invalidate_from(0);
    }

    // ---------------------------------------------------------------- queries

    pub fn rope(&self) -> &Rope {
        &self.rope
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }
    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }
    pub fn is_dirty(&self) -> bool {
        self.history.top_id() != self.saved_id || self.meta_dirty
    }
    pub fn is_read_only(&self) -> bool {
        self.user_read_only || self.file_read_only
    }
    pub fn can_undo(&self) -> bool {
        !self.history.undo.is_empty()
    }
    pub fn mark_meta_dirty(&mut self) {
        self.meta_dirty = true;
    }

    pub fn display_name(&self) -> String {
        match &self.file {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.to_string_lossy().to_string()),
            None => self.untitled_name.clone(),
        }
    }

    pub fn full_name(&self) -> String {
        match &self.file {
            Some(p) => p.to_string_lossy().to_string(),
            None => self.untitled_name.clone(),
        }
    }

    pub fn char_to_line(&self, pos: usize) -> usize {
        self.rope.char_to_line(pos.min(self.rope.len_chars()))
    }
    pub fn line_to_char(&self, line: usize) -> usize {
        self.rope.line_to_char(line.min(self.rope.len_lines()))
    }

    /// Char offset of the end of `line`, excluding the line break.
    pub fn line_end(&self, line: usize) -> usize {
        let start = self.line_to_char(line);
        start + self.line_len(line)
    }

    /// Length of a line in chars, excluding its line break.
    pub fn line_len(&self, line: usize) -> usize {
        if line >= self.rope.len_lines() {
            return 0;
        }
        let slice = self.rope.line(line);
        let mut len = slice.len_chars();
        if len > 0 && slice.char(len - 1) == '\n' {
            len -= 1;
        }
        if len > 0 && slice.char(len - 1) == '\r' {
            len -= 1;
        }
        len
    }

    /// Text of a line without its line break.
    pub fn line_text(&self, line: usize) -> String {
        if line >= self.rope.len_lines() {
            return String::new();
        }
        let slice = self.rope.line(line);
        let len = self.line_len(line);
        slice.slice(..len).to_string()
    }

    /// The EOL characters that terminate a line ("" for the last line).
    pub fn line_eol(&self, line: usize) -> &'static str {
        if line >= self.rope.len_lines() {
            return "";
        }
        let slice = self.rope.line(line);
        let n = slice.len_chars();
        if n >= 2 && slice.char(n - 2) == '\r' && slice.char(n - 1) == '\n' {
            "\r\n"
        } else if n >= 1 && slice.char(n - 1) == '\n' {
            "\n"
        } else if n >= 1 && slice.char(n - 1) == '\r' {
            "\r"
        } else {
            ""
        }
    }

    pub fn char_at(&self, pos: usize) -> Option<char> {
        if pos < self.rope.len_chars() {
            Some(self.rope.char(pos))
        } else {
            None
        }
    }

    pub fn slice_string(&self, range: Range<usize>) -> String {
        let len = self.rope.len_chars();
        let s = range.start.min(len);
        let e = range.end.min(len).max(s);
        self.rope.slice(s..e).to_string()
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Full text, cached until the next edit (used by search).
    pub fn text_arc(&mut self) -> Arc<String> {
        if let Some((v, t)) = &self.text_snapshot {
            if *v == self.version {
                return t.clone();
            }
        }
        let t = Arc::new(self.rope.to_string());
        self.text_snapshot = Some((self.version, t.clone()));
        t
    }

    pub fn byte_to_char(&self, byte: usize) -> usize {
        self.rope.byte_to_char(byte.min(self.rope.len_bytes()))
    }
    pub fn char_to_byte(&self, ch: usize) -> usize {
        self.rope.char_to_byte(ch.min(self.rope.len_chars()))
    }
    pub fn char_to_utf16(&self, ch: usize) -> usize {
        self.rope.char_to_utf16_cu(ch.min(self.rope.len_chars()))
    }
    pub fn utf16_to_char(&self, u: usize) -> usize {
        self.rope.utf16_cu_to_char(u.min(self.rope.len_utf16_cu()))
    }

    // ---------------------------------------------------------------- syntax

    /// Highlight (part of) `lines`; returns false when the time budget ran out.
    pub fn ensure_highlight(&mut self, lines: Range<usize>, theme: &syntect::highlighting::Theme, deadline: Instant) -> bool {
        self.highlighter.ensure(&self.rope, lines, theme, deadline)
    }

    pub fn set_syntax(&mut self, name: String) {
        let too_big = self.rope.len_bytes() > 64 * 1024 * 1024;
        self.highlighter = SyntaxHighlighter::new(if too_big { None } else { syntax::find_syntax(&name) });
        self.syntax_name = name;
    }

    // ---------------------------------------------------------------- editing

    /// Apply a batch of non-overlapping edits given in current coordinates.
    /// Records an undo transaction. Returns the edit infos (ascending, original coordinates).
    pub fn edit(
        &mut self,
        mut edits: Vec<(Range<usize>, String)>,
        kind: EditKind,
        sel_before: Vec<Selection>,
        origin: u64,
        cx: &mut Context<Self>,
    ) -> Arc<Vec<EditInfo>> {
        edits.sort_by_key(|(r, _)| (r.start, r.end));
        // Drop overlapping edits defensively.
        let mut cleaned: Vec<(Range<usize>, String)> = Vec::with_capacity(edits.len());
        let len = self.rope.len_chars();
        for (r, t) in edits {
            let r = r.start.min(len)..r.end.min(len).max(r.start.min(len));
            if let Some((last, _)) = cleaned.last() {
                if r.start < last.end {
                    continue;
                }
            }
            if r.is_empty() && t.is_empty() {
                continue;
            }
            cleaned.push((r, t));
        }
        if cleaned.is_empty() {
            return Arc::new(Vec::new());
        }
        let records: Vec<EditRecord> = cleaned
            .iter()
            .map(|(r, t)| EditRecord {
                start: r.start,
                old: self.rope.slice(r.clone()).to_string(),
                new: t.clone(),
                old_len: r.end - r.start,
                new_len: t.chars().count(),
            })
            .collect();
        let infos = self.apply_records(&records);

        // Record undo.
        let now = Instant::now();
        let merge = match self.history.undo.last() {
            Some(last) => {
                kind != EditKind::Other
                    && last.kind == kind
                    && last.mergeable
                    && last.id != self.saved_id
                    && now.duration_since(last.time).as_millis() < 1500
                    && sel_positions_eq(&last.sel_after, &sel_before)
                    && !(kind == EditKind::Typing
                        && records.iter().any(|r| r.new.contains('\n') || r.new.contains('\r')))
            }
            None => false,
        };
        self.history.redo.clear();
        let has_newline = records.iter().any(|r| r.new.contains('\n') || r.new.contains('\r'));
        if merge {
            let last = self.history.undo.last_mut().unwrap();
            last.batches.push(records);
            last.time = now;
        } else {
            let id = self.history.next_id;
            self.history.next_id += 1;
            self.history.undo.push(Transaction {
                id,
                batches: vec![records],
                sel_before,
                sel_after: Vec::new(),
                kind,
                time: now,
                mergeable: !(kind == EditKind::Typing && has_newline),
            });
            if self.history.undo.len() > 10_000 {
                self.history.undo.remove(0);
            }
        }
        cx.emit(BufferEvent::Edited { infos: infos.clone(), origin });
        cx.notify();
        infos
    }

    /// Record the selections after the last transaction (for redo).
    pub fn set_selections_after(&mut self, sels: Vec<Selection>) {
        if let Some(last) = self.history.undo.last_mut() {
            last.sel_after = sels;
        }
    }

    /// Stop the current undo transaction from absorbing further typing.
    pub fn break_undo_group(&mut self) {
        if let Some(last) = self.history.undo.last_mut() {
            last.mergeable = false;
        }
    }

    pub fn undo(&mut self, origin: u64, cx: &mut Context<Self>) -> Option<Vec<Selection>> {
        let tx = self.history.undo.pop()?;
        let mut all_infos = Vec::new();
        for batch in tx.batches.iter().rev() {
            // Inverse edits in post-batch coordinates.
            let mut delta: isize = 0;
            let inv: Vec<EditRecord> = batch
                .iter()
                .map(|r| {
                    let start = (r.start as isize + delta) as usize;
                    delta += r.new_len as isize - r.old_len as isize;
                    EditRecord {
                        start,
                        old: r.new.clone(),
                        new: r.old.clone(),
                        old_len: r.new_len,
                        new_len: r.old_len,
                    }
                })
                .collect();
            let infos = self.apply_records(&inv);
            all_infos.push(infos);
        }
        let sels = tx.sel_before.clone();
        self.history.redo.push(tx);
        for infos in all_infos {
            cx.emit(BufferEvent::Edited { infos, origin });
        }
        cx.notify();
        Some(sels)
    }

    pub fn redo(&mut self, origin: u64, cx: &mut Context<Self>) -> Option<Vec<Selection>> {
        let mut tx = self.history.redo.pop()?;
        let mut all_infos = Vec::new();
        for batch in tx.batches.iter() {
            all_infos.push(self.apply_records(batch));
        }
        tx.mergeable = false;
        let sels = tx.sel_after.clone();
        self.history.undo.push(tx);
        for infos in all_infos {
            cx.emit(BufferEvent::Edited { infos, origin });
        }
        cx.notify();
        Some(sels)
    }

    pub fn clear_history(&mut self) {
        self.history.undo.clear();
        self.history.redo.clear();
        self.saved_id = 0;
    }

    fn apply_records(&mut self, records: &[EditRecord]) -> Arc<Vec<EditInfo>> {
        let mut infos = Vec::with_capacity(records.len());
        for r in records {
            let start_line = self.rope.char_to_line(r.start);
            let old_end_line = self.rope.char_to_line(r.start + r.old_len);
            let new_line_count = count_line_breaks(&r.new);
            infos.push(EditInfo {
                start: r.start,
                old_len: r.old_len,
                new_len: r.new_len,
                start_line,
                old_end_line,
                new_line_count,
            });
        }
        for r in records.iter().rev() {
            if r.old_len > 0 {
                self.rope.remove(r.start..r.start + r.old_len);
            }
            if !r.new.is_empty() {
                self.rope.insert(r.start, &r.new);
            }
        }
        self.version += 1;
        if let Some(first) = infos.first() {
            // An edit can change the meaning of an earlier "\r" (CR + LF joining), so step back a line.
            self.highlighter.invalidate_from(first.start_line.saturating_sub(1));
        }
        self.adjust_line_markers(&infos);
        self.adjust_marks(&infos);
        // Record touched lines (final coordinates) in the change history.
        let mut shift: isize = 0;
        for e in &infos {
            let start = (e.start_line as isize + shift).max(0) as usize;
            for l in start..=start + e.new_line_count {
                self.change_history.insert(l, false);
            }
            shift += e.new_line_count as isize - (e.old_end_line - e.start_line) as isize;
        }
        Arc::new(infos)
    }

    fn adjust_line_markers(&mut self, infos: &[EditInfo]) {
        let line_delta_total: isize = infos
            .iter()
            .map(|e| e.new_line_count as isize - (e.old_end_line - e.start_line) as isize)
            .sum();
        if line_delta_total == 0 && infos.iter().all(|e| e.old_end_line == e.start_line) {
            return;
        }
        for e in infos.iter().rev() {
            let removed = e.old_end_line - e.start_line;
            let delta = e.new_line_count as isize - removed as isize;
            if removed == 0 && delta == 0 {
                continue;
            }
            let shift = |set: &BTreeSet<usize>| -> BTreeSet<usize> {
                set.iter()
                    .map(|&l| {
                        if l <= e.start_line {
                            l
                        } else if l <= e.old_end_line {
                            e.start_line
                        } else {
                            (l as isize + delta).max(0) as usize
                        }
                    })
                    .collect()
            };
            self.bookmarks = shift(&self.bookmarks);
            if !self.change_history.is_empty() {
                let mut moved = BTreeMap::new();
                for (&l, &saved) in self.change_history.iter() {
                    let nl = if l <= e.start_line {
                        l
                    } else if l <= e.old_end_line {
                        e.start_line
                    } else {
                        (l as isize + delta).max(0) as usize
                    };
                    let entry = moved.entry(nl).or_insert(saved);
                    *entry = *entry && saved;
                }
                self.change_history = moved;
            }
            for r in self.hidden_ranges.iter_mut() {
                let map = |l: usize| -> usize {
                    if l <= e.start_line {
                        l
                    } else if l <= e.old_end_line {
                        e.start_line
                    } else {
                        (l as isize + delta).max(0) as usize
                    }
                };
                *r = map(r.start)..map(r.end);
            }
        }
        self.hidden_ranges.retain(|r| r.end > r.start);
    }

    fn adjust_marks(&mut self, infos: &[EditInfo]) {
        if self.marks.is_empty() {
            return;
        }
        for m in self.marks.iter_mut() {
            let s = map_pos(m.range.start, infos, false);
            let e = map_pos(m.range.end, infos, false);
            m.range = s..e.max(s);
        }
        self.marks.retain(|m| !m.range.is_empty());
    }

    /// Replace the entire content (reload / re-decoding). Undoable.
    pub fn replace_all_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let len = self.rope.len_chars();
        self.edit(vec![(0..len, text.to_string())], EditKind::Other, vec![], 0, cx);
        cx.emit(BufferEvent::Reloaded);
    }

    // ---------------------------------------------------------------- files

    pub fn save_to(&mut self, path: &Path, cx: &mut Context<Self>) -> anyhow::Result<bool> {
        let text = self.rope.to_string();
        let (bytes, unmappable) = encoding::encode(&text, self.encoding);
        std::fs::write(path, &bytes)?;
        for v in self.change_history.values_mut() {
            *v = true;
        }
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let path = path.as_path();
        let path_changed = self.file.as_deref() != Some(path);
        self.file = Some(path.to_path_buf());
        self.saved_id = self.history.top_id();
        self.break_undo_group();
        self.meta_dirty = false;
        self.disk_missing = false;
        self.disk_mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        self.file_read_only = false;
        if path_changed && self.syntax_name == syntax::PLAIN_TEXT {
            let first = self.line_text(0);
            self.set_syntax(syntax::detect_syntax(path, &first, self.rope.len_bytes()));
        }
        cx.emit(BufferEvent::Saved);
        cx.notify();
        Ok(unmappable)
    }

    /// Write a copy without changing the buffer's file or dirty state.
    pub fn save_copy(&self, path: &Path) -> anyhow::Result<()> {
        let text = self.rope.to_string();
        let (bytes, _) = encoding::encode(&text, self.encoding);
        std::fs::write(path, bytes)?;
        Ok(())
    }

    pub fn reload_from_disk(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let Some(path) = self.file.clone() else { return Ok(()) };
        let loaded = load_file(&path)?;
        if loaded.text != self.rope.to_string() {
            self.replace_all_text(&loaded.text, cx);
        }
        self.change_history.clear();
        self.encoding = loaded.encoding;
        if let Some(eol) = loaded.eol {
            self.eol = eol;
        }
        self.saved_id = self.history.top_id();
        self.break_undo_group();
        self.meta_dirty = false;
        self.disk_mtime = loaded.mtime;
        self.disk_missing = false;
        self.file_read_only = loaded.read_only;
        cx.emit(BufferEvent::Saved);
        cx.notify();
        Ok(())
    }

    /// Re-interpret the file bytes using another encoding (Encoding > Encode in ...).
    pub fn reinterpret_encoding(&mut self, enc: TextEncoding, cx: &mut Context<Self>) {
        let bytes = match (&self.file, self.is_dirty()) {
            (Some(p), false) => std::fs::read(p).ok(),
            _ => None,
        };
        let bytes = bytes.unwrap_or_else(|| encoding::encode(&self.rope.to_string(), self.encoding).0);
        let text = encoding::decode_with(&bytes, enc);
        let was_dirty = self.is_dirty();
        if text != self.rope.to_string() {
            self.replace_all_text(&text, cx);
        }
        self.encoding = enc;
        if !was_dirty {
            self.saved_id = self.history.top_id();
        }
        cx.emit(BufferEvent::MetaChanged);
        cx.notify();
    }

    /// Change the encoding used for saving (Encoding > Convert to ...).
    pub fn convert_encoding(&mut self, enc: TextEncoding, cx: &mut Context<Self>) {
        if self.encoding != enc {
            self.encoding = enc;
            self.meta_dirty = true;
            cx.emit(BufferEvent::MetaChanged);
            cx.notify();
        }
    }

    /// Convert every line ending in the document (Edit > EOL Conversion).
    pub fn convert_eol(&mut self, eol: EolMode, cx: &mut Context<Self>) {
        let text = self.rope.to_string();
        let converted = encoding::normalize_eol(&text, eol);
        self.eol = eol;
        if converted != text {
            self.replace_all_text(&converted, cx);
        } else {
            self.meta_dirty = true;
        }
        cx.emit(BufferEvent::MetaChanged);
        cx.notify();
    }

    pub fn mark_saved_now(&mut self) {
        self.saved_id = self.history.top_id();
        self.meta_dirty = false;
    }

    pub fn mark_dirty_forced(&mut self) {
        self.saved_id = u64::MAX;
    }
}

fn sel_positions_eq(a: &[Selection], b: &[Selection]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.anchor == y.anchor && x.head == y.head)
}

pub fn count_line_breaks(s: &str) -> usize {
    let b = s.as_bytes();
    let mut n = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\n' => n += 1,
            b'\r' => {
                n += 1;
                if i + 1 < b.len() && b[i + 1] == b'\n' {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(start: usize, old_len: usize, new_len: usize) -> EditInfo {
        EditInfo { start, old_len, new_len, start_line: 0, old_end_line: 0, new_line_count: 0 }
    }

    #[test]
    fn map_positions() {
        let edits = vec![info(2, 0, 3), info(5, 2, 0)];
        assert_eq!(map_pos(0, &edits, false), 0);
        assert_eq!(map_pos(2, &edits, false), 2);
        assert_eq!(map_pos(2, &edits, true), 5);
        assert_eq!(map_pos(4, &edits, false), 7);
        assert_eq!(map_pos(6, &edits, false), 8);
        assert_eq!(map_pos(8, &edits, false), 9);
    }

    #[test]
    fn line_breaks() {
        assert_eq!(count_line_breaks("a\r\nb\nc\rd"), 3);
    }
}
