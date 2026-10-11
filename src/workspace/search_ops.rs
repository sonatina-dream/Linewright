//! Search menu commands and the logic behind the Find dialog buttons.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use gpui::{AppContext as _, ClipboardItem, Context, Entity, Focusable, PathPromptOptions, PromptLevel, Window};

use super::find_panel::*;
use super::Workspace;
use crate::actions::*;
use crate::buffer::{Buffer, EditKind, Mark, Selection};
use crate::editor::{Editor, EditorEvent};
use crate::search::{self, FileHits, Hit, ResultSet, SearchMode, SearchOptions, SearchQuery};
use crate::session::AppState;

impl Workspace {
    fn query(&mut self, cx: &mut Context<Self>) -> Option<(SearchQuery, SearchOptions)> {
        let (pattern, opts) = {
            let p = self.find_panel.read(cx);
            (p.pattern(cx), p.opts.clone())
        };
        if pattern.is_empty() {
            self.find_status("Type something to search for.", StatusKind::Warn, cx);
            return None;
        }
        AppState::push_history(&mut self.state.find_history, &pattern);
        match SearchQuery::new(&pattern, &opts) {
            Ok(q) => Some((q, opts)),
            Err(e) => {
                self.find_status(&e, StatusKind::Error, cx);
                if !self.find_visible {
                    self.notify_message(e, cx);
                }
                None
            }
        }
    }

    pub(crate) fn find_status(&mut self, msg: &str, kind: StatusKind, cx: &mut Context<Self>) {
        let msg = msg.to_string();
        self.find_panel.update(cx, |p, cx| p.set_status(msg, kind, cx));
        if !self.find_visible && kind != StatusKind::Ok {
            let m = self.find_panel.read(cx).status.clone();
            self.notify_message(m, cx);
        }
    }

    /// Find the next/previous match in `editor`. Returns true if something was found.
    pub(crate) fn find_step(&mut self, editor: &Entity<Editor>, q: &SearchQuery, forward: bool, wrap: bool, cx: &mut Context<Self>) -> bool {
        let text = editor.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.text_arc()));
        let (sel, b_len) = {
            let e = editor.read(cx);
            (e.primary_selection(), e.buf(cx).len_chars())
        };
        let _ = b_len;
        let found;
        let mut wrapped = false;
        {
            let b = editor.read(cx).buf(cx);
            if forward {
                let mut from = b.char_to_byte(sel.end());
                let mut m = q.find_at(&text, from, text.len());
                if let Some(r) = &m {
                    if r.is_empty() && r.start == from && sel.is_empty() {
                        from = next_boundary(&text, from);
                        m = if from <= text.len() { q.find_at(&text, from, text.len()) } else { None };
                    }
                }
                if m.is_none() && wrap {
                    m = q.find_at(&text, 0, text.len());
                    wrapped = m.is_some();
                }
                found = m;
            } else {
                let before = b.char_to_byte(sel.start());
                let mut m = q.find_prev(&text, before, 0..text.len());
                if m.is_none() && wrap {
                    m = q.find_prev(&text, text.len() + 1, 0..text.len());
                    wrapped = m.is_some();
                }
                found = m;
            }
        }
        match found {
            Some(r) => {
                let (s, e) = {
                    let b = editor.read(cx).buf(cx);
                    (b.byte_to_char(r.start), b.byte_to_char(r.end))
                };
                editor.update(cx, |ed, cx| ed.select_range(s..e, true, cx));
                if wrapped {
                    let msg = if forward {
                        "Reached the end of the document; continued from the top.".to_string()
                    } else {
                        "Reached the start of the document; continued from the bottom.".to_string()
                    };
                    self.find_status(&msg, StatusKind::Warn, cx);
                } else {
                    self.find_status("", StatusKind::Info, cx);
                }
                true
            }
            None => {
                let pat = q.pattern.clone();
                self.find_status(&format!("No matches for \"{pat}\"."), StatusKind::Error, cx);
                false
            }
        }
    }

    fn find_dir(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let Some(editor) = self.active_editor() else { return false };
        let Some((q, opts)) = self.query(cx) else { return false };
        self.record_workspace_step(if forward { Box::new(FindNext) } else { Box::new(FindPrev) }, cx);
        self.find_step(&editor, &q, forward, opts.wrap_around, cx)
    }

    fn record_workspace_step(&self, action: Box<dyn gpui::Action>, cx: &mut Context<Self>) {
        if let Some(r) = cx.try_global::<crate::editor::MacroRecorder>() {
            if r.recording {
                cx.global_mut::<crate::editor::MacroRecorder>().steps.push(crate::editor::MacroStep::Action(action));
            }
        }
    }

    // ------------------------------------------------------------------ dialog opening

    fn show_find_panel(&mut self, tab: FindTab, window: &mut Window, cx: &mut Context<Self>) {
        let fill = cx.global::<crate::settings::Settings>().find_dialog_fill_from_selection;
        if fill {
            if let Some(e) = self.active_editor() {
                let ed = e.read(cx);
                let mut text = ed.selected_text(cx);
                if text.is_empty() {
                    text = crate::editor::word_at_pub(ed.buf(cx), ed.caret()).map(|r| ed.buf(cx).slice_string(r)).unwrap_or_default();
                }
                if !text.is_empty() && !text.contains(['\n', '\r']) && text.chars().count() < 1024 {
                    self.find_panel.update(cx, |p, cx| p.set_pattern(&text, window, cx));
                }
            }
        }
        self.find_panel.update(cx, |p, cx| {
            p.tab = tab;
            cx.notify();
        });
        if tab == FindTab::FindInFiles && self.find_panel.read(cx).dir_input.read(cx).buf(cx).len_chars() == 0 {
            self.fp_use_current_dir(&FpUseCurrentDir, window, cx);
        }
        self.find_visible = true;
        let panel = self.find_panel.clone();
        panel.update(cx, |p, cx| p.focus_find(window, cx));
        cx.notify();
    }

    pub fn find(&mut self, _: &Find, window: &mut Window, cx: &mut Context<Self>) {
        self.show_find_panel(FindTab::Find, window, cx);
    }
    pub fn replace(&mut self, _: &Replace, window: &mut Window, cx: &mut Context<Self>) {
        self.show_find_panel(FindTab::Replace, window, cx);
    }
    pub fn find_in_files(&mut self, _: &FindInFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.show_find_panel(FindTab::FindInFiles, window, cx);
    }
    pub fn mark_dialog(&mut self, _: &MarkDialog, window: &mut Window, cx: &mut Context<Self>) {
        self.show_find_panel(FindTab::Mark, window, cx);
    }
    pub fn close_find_panel(&mut self, _: &CloseFindPanel, window: &mut Window, cx: &mut Context<Self>) {
        self.find_visible = false;
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(crate) fn render_find_overlay(&mut self, cx: &mut Context<Self>) -> gpui::AnyElement {
        use gpui::prelude::*;
        let pos = self.find_pos;
        let panel = self.find_panel.clone();
        let el = gpui::div().absolute();
        let el = match pos {
            Some(p) => el.left(p.x).top(p.y),
            None => el.top(gpui::px(70.0)).right(gpui::px(36.0)),
        };
        el.child(panel)
            .on_drag_move(cx.listener(|this, e: &gpui::DragMoveEvent<FindDrag>, _, cx| {
                let grab = e.drag(cx).grab.get();
                let p = e.event.position - grab;
                this.find_pos = Some(gpui::point(p.x.max(gpui::px(0.0)), p.y.max(gpui::px(0.0))));
                cx.notify();
            }))
            .into_any_element()
    }

    // ------------------------------------------------------------------ Find tab buttons

    pub fn find_next(&mut self, _: &FindNext, _: &mut Window, cx: &mut Context<Self>) {
        let backward = self.find_panel.read(cx).opts.backward;
        self.find_dir(!backward, cx);
    }
    pub fn find_prev(&mut self, _: &FindPrev, _: &mut Window, cx: &mut Context<Self>) {
        let backward = self.find_panel.read(cx).opts.backward;
        self.find_dir(backward, cx);
    }
    pub(crate) fn fp_find_next(&mut self, _: &FpFindNext, _: &mut Window, cx: &mut Context<Self>) {
        let backward = self.find_panel.read(cx).opts.backward;
        self.find_dir(!backward, cx);
    }
    pub(crate) fn fp_find_prev(&mut self, _: &FpFindPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.find_dir(false, cx);
    }

    fn search_range(&self, editor: &Entity<Editor>, opts: &SearchOptions, text: &str, cx: &gpui::App) -> (Range<usize>, bool) {
        let e = editor.read(cx);
        let sel = e.primary_selection();
        if opts.in_selection && !sel.is_empty() {
            let b = e.buf(cx);
            (b.char_to_byte(sel.start())..b.char_to_byte(sel.end()), true)
        } else {
            (0..text.len(), false)
        }
    }

    pub(crate) fn fp_count(&mut self, _: &FpCount, _: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else { return };
        let Some((q, opts)) = self.query(cx) else { return };
        let text = editor.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.text_arc()));
        let (range, in_sel) = self.search_range(&editor, &opts, &text, cx);
        let n = q.find_all(&text, range, None).len();
        self.find_status(
            &format!("{n} match{} in {}.", if n == 1 { "" } else { "es" }, if in_sel { "the selection" } else { "the whole document" }),
            StatusKind::Ok,
            cx,
        );
    }

    fn result_set_for(&self, editor: &Entity<Editor>, q: &SearchQuery, opts: &SearchOptions, cx: &mut Context<Self>) -> Option<FileHits> {
        let text = editor.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.text_arc()));
        let (range, _) = self.search_range(editor, opts, &text, cx);
        let matches = q.find_all(&text, range, None);
        if matches.is_empty() {
            return None;
        }
        let b = editor.read(cx).buf(cx);
        Some(FileHits { path: b.file.clone(), buffer_id: Some(b.id), name: b.full_name(), hits: search::hits_from_matches(&text, &matches) })
    }

    pub(crate) fn fp_find_all_current(&mut self, _: &FpFindAllCurrent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else { return };
        let Some((q, opts)) = self.query(cx) else { return };
        let files: Vec<FileHits> = self.result_set_for(&editor, &q, &opts, cx).into_iter().collect();
        self.publish_results(&q.pattern, files, "current document", cx);
    }

    pub(crate) fn fp_find_all_open(&mut self, _: &FpFindAllOpen, _: &mut Window, cx: &mut Context<Self>) {
        let Some((q, mut opts)) = self.query(cx) else { return };
        opts.in_selection = false;
        let mut files = Vec::new();
        for (e, _) in self.unique_buffers(cx) {
            if let Some(f) = self.result_set_for(&e, &q, &opts, cx) {
                files.push(f);
            }
        }
        self.publish_results(&q.pattern, files, "all opened documents", cx);
    }

    fn publish_results(&mut self, pattern: &str, files: Vec<FileHits>, scope: &str, cx: &mut Context<Self>) {
        let total: usize = files.iter().map(|f| f.hits.iter().map(|h| h.ranges.len()).sum::<usize>()).sum();
        let nfiles = files.len();
        let title = format!("\"{pattern}\": {total} match{} in {nfiles} file{} ({scope})", if total == 1 { "" } else { "es" }, if nfiles == 1 { "" } else { "s" });
        self.results.update(cx, |r, cx| r.push(ResultSet { title, files, total, collapsed: false }, cx));
        self.results_visible = true;
        self.find_status(&format!("Found {total} match{}.", if total == 1 { "" } else { "es" }), if total > 0 { StatusKind::Ok } else { StatusKind::Error }, cx);
        cx.notify();
    }

    // ------------------------------------------------------------------ Replace

    pub(crate) fn fp_replace(&mut self, _: &FpReplace, _: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else { return };
        let Some((q, opts)) = self.query(cx) else { return };
        let replacement = self.find_panel.read(cx).replacement(cx);
        AppState::push_history(&mut self.state.replace_history, &replacement);
        let text = editor.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.text_arc()));
        let sel = editor.read(cx).primary_selection();
        let (sb, eb) = {
            let b = editor.read(cx).buf(cx);
            (b.char_to_byte(sel.start()), b.char_to_byte(sel.end()))
        };
        // If the selection is exactly a match, replace it.
        if let Some(m) = q.find_at(&text, sb, text.len()) {
            if m.start == sb && m.end == eb && (!sel.is_empty() || m.is_empty()) {
                let new = q.expand_replacement(&text, &m, &replacement);
                editor.update(cx, |e, cx| {
                    let start = sel.start();
                    let n = new.chars().count();
                    e.transact(vec![(sel.range(), new)], EditKind::Other, cx);
                    e.set_selections(vec![Selection::caret(start + n)], cx);
                    e.finish_transaction(cx);
                });
            }
        }
        if !self.find_step(&editor, &q, !opts.backward, opts.wrap_around, cx) {
            self.find_status("No more matches.", StatusKind::Warn, cx);
        }
    }

    /// Replace all matches in one editor; returns the count.
    fn replace_all_in(&mut self, editor: &Entity<Editor>, q: &SearchQuery, opts: &SearchOptions, replacement: &str, cx: &mut Context<Self>) -> usize {
        if editor.read(cx).buf(cx).is_read_only() {
            return 0;
        }
        let text = editor.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.text_arc()));
        let (range, _) = self.search_range(editor, opts, &text, cx);
        let matches = q.find_all(&text, range, None);
        if matches.is_empty() {
            return 0;
        }
        let n = matches.len();
        editor.update(cx, |e, cx| {
            let b = e.buf(cx);
            let edits: Vec<(Range<usize>, String)> = if n <= 50_000 {
                matches
                    .iter()
                    .map(|m| (b.byte_to_char(m.start)..b.byte_to_char(m.end), q.expand_replacement(&text, m, replacement)))
                    .collect()
            } else {
                let new_text = search::replace_matches(&text, &matches, q, replacement);
                vec![(0..b.len_chars(), new_text)]
            };
            let caret = e.caret();
            e.transact(edits, EditKind::Other, cx);
            let len = e.buf(cx).len_chars();
            e.set_selections(vec![Selection::caret(caret.min(len))], cx);
            e.finish_transaction(cx);
        });
        n
    }

    pub(crate) fn fp_replace_all(&mut self, _: &FpReplaceAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else { return };
        let Some((q, opts)) = self.query(cx) else { return };
        let replacement = self.find_panel.read(cx).replacement(cx);
        AppState::push_history(&mut self.state.replace_history, &replacement);
        let in_sel = opts.in_selection && editor.read(cx).has_selection();
        let n = self.replace_all_in(&editor, &q, &opts, &replacement, cx);
        self.find_status(
            &format!("Replaced {n} match{} in {}.", if n == 1 { "" } else { "es" }, if in_sel { "the selection" } else { "the whole document" }),
            if n > 0 { StatusKind::Ok } else { StatusKind::Error },
            cx,
        );
    }

    pub(crate) fn fp_replace_all_open(&mut self, _: &FpReplaceAllOpen, _: &mut Window, cx: &mut Context<Self>) {
        let Some((q, mut opts)) = self.query(cx) else { return };
        opts.in_selection = false;
        let replacement = self.find_panel.read(cx).replacement(cx);
        let mut total = 0;
        let mut docs = 0;
        for (e, _) in self.unique_buffers(cx) {
            let n = self.replace_all_in(&e, &q, &opts, &replacement, cx);
            if n > 0 {
                docs += 1;
                total += n;
            }
        }
        self.find_status(&format!("Replaced {total} matches in {docs} open documents."), StatusKind::Ok, cx);
    }

    // ------------------------------------------------------------------ Find in Files

    pub(crate) fn fp_use_current_dir(&mut self, _: &FpUseCurrentDir, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self
            .active_editor()
            .and_then(|e| e.read(cx).buf(cx).file.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())))
            .or_else(|| self.folder.read(cx).roots.first().cloned())
            .or_else(dirs::home_dir)
            .unwrap_or_default();
        let s = dir.to_string_lossy().to_string();
        let input = self.find_panel.read(cx).dir_input.clone();
        input.update(cx, |e, cx| e.set_text(&s, window, cx));
    }

    pub(crate) fn fp_browse_dir(&mut self, _: &FpBrowseDir, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Choose".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(p) = paths.first() {
                    let s = p.to_string_lossy().to_string();
                    let _ = this.update_in(cx, |this, window, cx| {
                        let input = this.find_panel.read(cx).dir_input.clone();
                        input.update(cx, |e, cx| e.set_text(&s, window, cx));
                    });
                }
            }
        })
        .detach();
    }

    fn fif_params(&mut self, cx: &mut Context<Self>) -> Option<(SearchQuery, search::FileSearchParams)> {
        let (q, _) = self.query(cx)?;
        let p = self.find_panel.read(cx);
        let dir = p.dir_input.read(cx).text(cx);
        let filters = p.filters_input.read(cx).text(cx);
        let params = search::FileSearchParams {
            directory: PathBuf::from(dir.trim()),
            filters: if filters.trim().is_empty() { "*.*".into() } else { filters },
            recursive: p.recursive,
            include_hidden: p.hidden,
        };
        if !params.directory.is_dir() {
            self.find_status("That folder doesn't exist.", StatusKind::Error, cx);
            return None;
        }
        AppState::push_history(&mut self.state.find_dirs, &params.directory.to_string_lossy());
        AppState::push_history(&mut self.state.find_filters, &params.filters);
        Some((q, params))
    }

    pub(crate) fn fp_find_in_files(&mut self, _: &FpFindInFiles, window: &mut Window, cx: &mut Context<Self>) {
        let Some((q, params)) = self.fif_params(cx) else { return };
        let cancel = Arc::new(AtomicBool::new(false));
        self.results.update(cx, |r, cx| {
            r.busy = Some("Searching…".into());
            r.cancel = Some(cancel.clone());
            cx.notify();
        });
        self.results_visible = true;
        let pattern = q.pattern.clone();
        let dir = params.directory.clone();
        let task = cx.background_executor().spawn(async move {
            let files = search::collect_files(&params, &cancel);
            let n = files.len();
            let hits = search::search_in_files(&files, &q, &cancel, &|_| {});
            (hits, n)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (hits, scanned) = task.await;
            let _ = this.update(cx, |this, cx| {
                this.results.update(cx, |r, cx| {
                    r.busy = None;
                    r.cancel = None;
                    cx.notify();
                });
                let scope = format!("{} searched file{} in {}", scanned, if scanned == 1 { "" } else { "s" }, dir.display());
                this.publish_results(&pattern, hits, &scope, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn fp_replace_in_files(&mut self, _: &FpReplaceInFiles, window: &mut Window, cx: &mut Context<Self>) {
        let Some((q, params)) = self.fif_params(cx) else { return };
        let replacement = self.find_panel.read(cx).replacement(cx);
        let answer = window.prompt(
            PromptLevel::Warning,
            "Replace in all matching files?",
            Some(&format!("Folder: {}\nFilters: {}\n\nThis changes files on disk and can't be undone.", params.directory.display(), params.filters)),
            &["Replace", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if !matches!(answer.await, Ok(0)) {
                return;
            }
            let cancel = Arc::new(AtomicBool::new(false));
            let (files_changed, count) = cx
                .background_executor()
                .spawn(async move {
                    let files = search::collect_files(&params, &cancel);
                    search::replace_in_files(&files, &q, &replacement, &cancel)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.find_status(&format!("Replaced {count} matches in {files_changed} files."), StatusKind::Ok, cx);
                this.check_files_changed(window, cx);
            });
        })
        .detach();
    }

    // ------------------------------------------------------------------ Mark

    pub(crate) fn fp_mark_all(&mut self, _: &FpMarkAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else { return };
        let Some((q, opts)) = self.query(cx) else { return };
        let (bookmark, purge) = {
            let p = self.find_panel.read(cx);
            (p.bookmark_line, p.purge)
        };
        let text = editor.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.text_arc()));
        let (range, _) = self.search_range(&editor, &opts, &text, cx);
        let matches = q.find_all(&text, range, None);
        let n = matches.len();
        let buffer = editor.read(cx).buffer.clone();
        buffer.update(cx, |b: &mut Buffer, cx| {
            if purge {
                b.marks.retain(|m| m.style != 0);
                if bookmark {
                    b.bookmarks.clear();
                }
            }
            for m in &matches {
                let s = b.byte_to_char(m.start);
                let e = b.byte_to_char(m.end);
                b.marks.push(Mark { range: s..e, style: 0 });
                if bookmark {
                    let l = b.char_to_line(s);
                    b.bookmarks.insert(l);
                }
            }
            b.marks.sort_by_key(|m| m.range.start);
            cx.notify();
        });
        editor.update(cx, |_, cx| cx.notify());
        self.find_status(&format!("Marked {n} match{}.", if n == 1 { "" } else { "es" }), if n > 0 { StatusKind::Ok } else { StatusKind::Error }, cx);
    }

    pub(crate) fn fp_clear_marks(&mut self, _: &FpClearMarks, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor() {
            let buffer = editor.read(cx).buffer.clone();
            buffer.update(cx, |b, cx| {
                b.marks.retain(|m| m.style != 0);
                cx.notify();
            });
            editor.update(cx, |_, cx| cx.notify());
        }
        self.find_status("Marks cleared.", StatusKind::Info, cx);
    }

    pub(crate) fn fp_copy_marked(&mut self, _: &FpCopyMarked, _: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else { return };
        let b = editor.read(cx).buf(cx);
        let eol = b.eol.as_str();
        let parts: Vec<String> = b.marks.iter().filter(|m| m.style == 0).map(|m| b.slice_string(m.range.clone())).collect();
        if parts.is_empty() {
            self.find_status("Nothing is marked.", StatusKind::Warn, cx);
            return;
        }
        let text = parts.join(eol);
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        crate::editor::push_clipboard_history(cx, &text);
        self.find_status(&format!("Copied {} marked items.", parts.len()), StatusKind::Ok, cx);
    }

    pub fn mark_all_occurrences(&mut self, _: &MarkAllOccurrences, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            let ed = e.read(cx);
            let text = ed.selected_text(cx);
            let text = if text.is_empty() {
                crate::editor::word_at_pub(ed.buf(cx), ed.caret()).map(|r| ed.buf(cx).slice_string(r)).unwrap_or_default()
            } else {
                text
            };
            if text.is_empty() {
                return;
            }
            self.find_panel.update(cx, |p, cx| p.set_pattern(&text, w, cx));
            self.fp_mark_all(&FpMarkAll, w, cx);
        }
    }

    // ------------------------------------------------------------------ select & find / volatile

    fn selection_or_word(&self, cx: &gpui::App) -> Option<String> {
        let e = self.active_editor()?;
        let ed = e.read(cx);
        let s = ed.selected_text(cx);
        if !s.is_empty() {
            return Some(s);
        }
        let b = ed.buf(cx);
        crate::editor::word_at_pub(b, ed.caret()).map(|r| b.slice_string(r))
    }

    fn quick_find(&mut self, set_pattern: bool, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.selection_or_word(cx) else { return };
        let Some(editor) = self.active_editor() else { return };
        if set_pattern {
            self.find_panel.update(cx, |p, cx| {
                p.set_pattern(&text, window, cx);
                p.opts.mode = SearchMode::Normal;
            });
        }
        let opts = SearchOptions { mode: SearchMode::Normal, match_case: self.find_panel.read(cx).opts.match_case, wrap_around: true, ..Default::default() };
        if let Ok(q) = SearchQuery::new(&text, &opts) {
            // Make sure the current word counts as "current match".
            let ed = editor.read(cx);
            if ed.primary_selection().is_empty() {
                if let Some(r) = crate::editor::word_at_pub(ed.buf(cx), ed.caret()) {
                    editor.update(cx, |e, cx| e.set_selections(vec![Selection::new(r.start, r.end)], cx));
                }
            }
            self.find_step(&editor, &q, forward, true, cx);
        }
    }

    pub fn select_and_find_next(&mut self, _: &SelectAndFindNext, w: &mut Window, cx: &mut Context<Self>) {
        self.record_workspace_step(Box::new(SelectAndFindNext), cx);
        self.quick_find(true, true, w, cx);
    }
    pub fn select_and_find_prev(&mut self, _: &SelectAndFindPrev, w: &mut Window, cx: &mut Context<Self>) {
        self.record_workspace_step(Box::new(SelectAndFindPrev), cx);
        self.quick_find(true, false, w, cx);
    }
    pub fn find_word_next(&mut self, _: &FindWordNext, w: &mut Window, cx: &mut Context<Self>) {
        self.quick_find(false, true, w, cx);
    }
    pub fn find_word_prev(&mut self, _: &FindWordPrev, w: &mut Window, cx: &mut Context<Self>) {
        self.quick_find(false, false, w, cx);
    }

    pub fn use_selection_for_find(&mut self, _: &UseSelectionForFind, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selection_or_word(cx) {
            self.find_panel.update(cx, |p, cx| p.set_pattern(&text, window, cx));
            self.notify_message(format!("Search text: \"{text}\""), cx);
        }
    }

    // ------------------------------------------------------------------ incremental search

    pub fn incremental_search(&mut self, _: &IncrementalSearch, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(inc) = &self.incremental {
            window.focus(&inc.focus_handle(cx));
            return;
        }
        let input = cx.new(|cx| Editor::single_line("Incremental search", window, cx));
        let start = self.active_editor().map(|e| e.read(cx).primary_selection().start()).unwrap_or(0);
        let sub = cx.subscribe_in(&input, window, move |this, input, ev: &EditorEvent, window, cx| match ev {
            EditorEvent::Edited => this.incremental_update(input.read(cx).text(cx), start, cx),
            EditorEvent::Confirm { shift } => this.incremental_step(!*shift, window, cx),
            EditorEvent::Cancel => this.close_incremental(window, cx),
            _ => {}
        });
        self.incremental_sub = Some(sub);
        window.focus(&input.focus_handle(cx));
        self.incremental = Some(input);
        self.incremental_status = "".into();
        cx.notify();
    }

    fn incremental_update(&mut self, pattern: String, start: usize, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else { return };
        if pattern.is_empty() {
            editor.update(cx, |e, cx| {
                e.incremental_highlight.clear();
                cx.notify();
            });
            self.incremental_status = "".into();
            cx.notify();
            return;
        }
        let opts = SearchOptions { match_case: self.find_panel.read(cx).opts.match_case, ..Default::default() };
        let Ok(q) = SearchQuery::new(&pattern, &opts) else { return };
        let text = editor.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.text_arc()));
        let matches = q.find_all(&text, 0..text.len(), None);
        let (ranges, first) = {
            let b = editor.read(cx).buf(cx);
            let ranges: Vec<Range<usize>> = matches.iter().take(20_000).map(|m| b.byte_to_char(m.start)..b.byte_to_char(m.end)).collect();
            let first = ranges.iter().find(|r| r.start >= start).or(ranges.first()).cloned();
            (ranges, first)
        };
        let n = matches.len();
        editor.update(cx, |e, cx| {
            e.incremental_highlight = ranges;
            if let Some(r) = first {
                e.select_range(r, true, cx);
            }
            cx.notify();
        });
        self.incremental_status = if n == 0 { "No matches".into() } else { format!("{n} matches").into() };
        cx.notify();
    }

    pub(crate) fn incremental_step(&mut self, forward: bool, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(inc) = self.incremental.clone() else { return };
        let pattern = inc.read(cx).text(cx);
        let Some(editor) = self.active_editor() else { return };
        let opts = SearchOptions { match_case: self.find_panel.read(cx).opts.match_case, ..Default::default() };
        if let Ok(q) = SearchQuery::new(&pattern, &opts) {
            self.find_step(&editor, &q, forward, true, cx);
        }
    }

    pub(crate) fn close_incremental(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.incremental = None;
        self.incremental_sub = None;
        if let Some(e) = self.active_editor() {
            e.update(cx, |e, cx| {
                e.incremental_highlight.clear();
                cx.notify();
            });
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    // ------------------------------------------------------------------ go to / results navigation

    pub fn go_to_line(&mut self, _: &GoToLine, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let view = cx.new(|cx| super::dialogs::GoToDialog::new(self.weak_self.clone(), e.clone(), window, cx));
        self.open_modal(view, window, cx);
    }

    pub fn show_search_results(&mut self, _: &ShowSearchResults, _: &mut Window, cx: &mut Context<Self>) {
        self.results_visible = true;
        cx.notify();
    }

    pub fn next_search_result(&mut self, _: &NextSearchResult, window: &mut Window, cx: &mut Context<Self>) {
        self.results_visible = true;
        self.results.update(cx, |r, cx| r.step(true, window, cx));
    }

    pub fn prev_search_result(&mut self, _: &PrevSearchResult, window: &mut Window, cx: &mut Context<Self>) {
        self.results_visible = true;
        self.results.update(cx, |r, cx| r.step(false, window, cx));
    }

    pub fn find_chars_in_range(&mut self, _: &FindCharsInRange, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| super::dialogs::CharRangeDialog::new(self.weak_self.clone(), window, cx));
        self.open_modal(view, window, cx);
    }

    /// Find the next char whose code point is in `range` (Search > Find Characters by Code).
    pub(crate) fn find_char_range(&mut self, range: std::ops::RangeInclusive<u32>, forward: bool, cx: &mut Context<Self>) -> bool {
        let Some(editor) = self.active_editor() else { return false };
        let (sel, rope) = {
            let e = editor.read(cx);
            (e.primary_selection(), e.buf(cx).rope().clone())
        };
        let len = rope.len_chars();
        let found = if forward {
            let start = sel.end();
            (start..len).chain(0..start).find(|&i| range.contains(&(rope.char(i) as u32)))
        } else {
            let start = sel.start();
            (0..start).rev().chain((start..len).rev()).find(|&i| range.contains(&(rope.char(i) as u32)))
        };
        match found {
            Some(i) => {
                editor.update(cx, |e, cx| e.select_range(i..i + 1, true, cx));
                true
            }
            None => {
                self.notify_message("No characters found in that range.", cx);
                false
            }
        }
    }

    /// Open a search hit (from the results panel).
    pub fn open_hit(&mut self, path: Option<PathBuf>, buffer_id: Option<u64>, hit: &Hit, window: &mut Window, cx: &mut Context<Self>) {
        let editor = if let Some(id) = buffer_id {
            self.all_editors().into_iter().find(|e| e.read(cx).buf(cx).id == id)
        } else {
            None
        };
        let editor = match editor {
            Some(e) => {
                if let Some((pi, i)) = self.find_editor_location(&e) {
                    self.activate(pi, i, window, cx);
                }
                Some(e)
            }
            None => match path {
                Some(p) => self.open_path(&p, window, cx),
                None => None,
            },
        };
        let Some(editor) = editor else { return };
        let (start, end) = (hit.start, hit.end);
        editor.update(cx, |e, cx| {
            let len = e.buf(cx).len_chars();
            e.select_range(start.min(len)..end.min(len), true, cx);
        });
        window.focus(&editor.focus_handle(cx));
    }
}

fn next_boundary(text: &str, pos: usize) -> usize {
    let mut p = pos + 1;
    while p < text.len() && !text.is_char_boundary(p) {
        p += 1;
    }
    p
}
