//! File menu operations, file monitoring, backups and session persistence.

use std::path::{Path, PathBuf};

use gpui::{AppContext as _, ClipboardItem, Context, Entity, Focusable, PathPromptOptions, PromptLevel, Task, Window};

use super::Workspace;
use crate::actions::*;
use crate::buffer::{self, Buffer, Selection};
use crate::editor::Editor;
use crate::encoding::{EolMode, TextEncoding};
use crate::session::{Session, SessionFile, SessionPane};
use crate::settings::Settings;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SaveMode {
    Save,
    SaveAs,
    SaveCopy,
}

impl Workspace {
    fn default_dir(&self, cx: &gpui::App) -> PathBuf {
        if let Some(e) = self.active_editor() {
            if let Some(p) = e.read(cx).buf(cx).file.as_ref().and_then(|p| p.parent()) {
                return p.to_path_buf();
            }
        }
        let d = &cx.global::<Settings>().default_directory;
        if !d.is_empty() && Path::new(d).is_dir() {
            return PathBuf::from(d);
        }
        dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from("/"))
    }

    pub fn new_buffer_entity(&mut self, cx: &mut Context<Self>) -> Entity<Buffer> {
        let id = self.alloc_buffer_id();
        let name = self.next_untitled_name(cx);
        let s = cx.global::<Settings>();
        let eol = EolMode::from_id(&s.new_doc_eol);
        let enc = TextEncoding::from_id(&s.new_doc_encoding).unwrap_or_default();
        let lang = s.new_doc_language.clone();
        cx.new(|_| {
            let mut b = Buffer::new(id, name, eol, enc);
            b.set_syntax(lang);
            b
        })
    }

    pub fn new_file(&mut self, _: &NewFile, window: &mut Window, cx: &mut Context<Self>) {
        let buffer = self.new_buffer_entity(cx);
        let pane = if self.panes[self.active_pane].items.is_empty() { 0 } else { self.active_pane };
        self.add_buffer(buffer, pane, true, window, cx);
    }

    pub fn open_file(&mut self, _: &OpenFile, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Open".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| this.open_paths(paths, window, cx));
            }
        })
        .detach();
    }

    pub fn open_folder(&mut self, _: &OpenFolder, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Open Folder".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update_in(cx, |this, window, cx| {
                    for p in paths {
                        this.folder.update(cx, |f, cx| f.add_root(p, cx));
                    }
                    this.left_panel = Some(super::LeftPanel::Folder);
                    let _ = window;
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Open files (or folders, which go to the Project Folder panel).
    pub fn open_paths(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let mut last = None;
        for p in paths {
            if p.is_dir() {
                self.folder.update(cx, |f, cx| f.add_root(p.clone(), cx));
                self.left_panel = Some(super::LeftPanel::Folder);
                continue;
            }
            if let Some(e) = self.open_path(&p, window, cx) {
                last = Some(e);
            }
        }
        if let Some(e) = last {
            if let Some((pi, i)) = self.find_editor_location(&e) {
                self.activate(pi, i, window, cx);
            }
        }
        cx.notify();
    }

    pub fn open_path(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<Editor>> {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some((pi, i)) = self.editor_for_path(&path, cx) {
            self.activate(pi, i, window, cx);
            return self.panes[pi].items.get(i).cloned();
        }
        if !path.exists() {
            // Offer to create files that don't exist yet.
            let p = path.clone();
            let answer = window.prompt(
                PromptLevel::Info,
                &format!("\"{}\" doesn't exist. Create it?", path.display()),
                None,
                &["Yes", "No"],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                if let Ok(0) = answer.await {
                    if std::fs::write(&p, b"").is_ok() {
                        let _ = this.update_in(cx, |this, window, cx| {
                            this.open_path(&p, window, cx);
                        });
                    }
                }
            })
            .detach();
            return None;
        }
        let loaded = match buffer::load_file(&path) {
            Ok(l) => l,
            Err(e) => {
                self.notify_message(format!("Cannot open {}: {e}", path.display()), cx);
                return None;
            }
        };
        let id = self.alloc_buffer_id();
        let default_eol = EolMode::from_id(&cx.global::<Settings>().new_doc_eol);
        let p2 = path.clone();
        let buffer = cx.new(|_| Buffer::from_loaded(id, p2, loaded, default_eol));
        // Replace a lone, empty, untouched "new 1" tab.
        let pane = self.active_pane;
        let replace = {
            let p = &self.panes[pane];
            p.items.len() == 1 && {
                let b = p.items[0].read(cx).buf(cx);
                b.file.is_none() && b.len_chars() == 0 && !b.is_dirty() && !b.can_undo()
            }
        };
        let editor = self.add_buffer(buffer, pane, true, window, cx);
        if replace {
            let idx = self.panes[pane].items.iter().position(|e| *e != editor);
            if let Some(i) = idx {
                let old = self.panes[pane].items.remove(i);
                self.editor_subs.remove(&old.entity_id());
                self.panes[pane].active = self.panes[pane].items.iter().position(|e| *e == editor).unwrap_or(0);
            }
        }
        let max = cx.global::<Settings>().max_recent_files;
        self.state.push_recent(&path, max);
        self.state.save();
        cx.add_recent_document(&path);
        self.refresh_menus(window, cx);
        Some(editor)
    }

    // ------------------------------------------------------------------ saving

    pub fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            self.save_editor(e, SaveMode::Save, window, cx).detach();
        }
    }

    pub fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            self.save_editor(e, SaveMode::SaveAs, window, cx).detach();
        }
    }

    pub fn save_copy_as(&mut self, _: &SaveCopyAs, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            self.save_editor(e, SaveMode::SaveCopy, window, cx).detach();
        }
    }

    pub fn save_all(&mut self, _: &SaveAll, window: &mut Window, cx: &mut Context<Self>) {
        let editors: Vec<Entity<Editor>> = self
            .unique_buffers(cx)
            .into_iter()
            .filter(|(_, b)| b.read(cx).is_dirty())
            .map(|(e, _)| e)
            .collect();
        cx.spawn_in(window, async move |this, cx| {
            for e in editors {
                let Ok(task) = this.update_in(cx, |this, window, cx| this.save_editor(e.clone(), SaveMode::Save, window, cx)) else {
                    return;
                };
                if !task.await {
                    return;
                }
            }
        })
        .detach();
    }

    pub fn save_editor(&mut self, editor: Entity<Editor>, mode: SaveMode, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        let (path, name) = {
            let b = editor.read(cx).buf(cx);
            (b.file.clone(), b.display_name())
        };
        if mode == SaveMode::Save {
            if let Some(p) = path.clone() {
                return Task::ready(self.write_editor(&editor, &p, cx));
            }
        }
        let dir = path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| self.default_dir(cx));
        let suggested = if path.is_some() {
            name
        } else {
            let first: String = editor.read(cx).buf(cx).line_text(0).chars().take(40).collect();
            let first = first.trim().replace(['/', ':', '\\'], "_");
            if first.is_empty() { format!("{name}.txt") } else { format!("{first}.txt") }
        };
        let rx = cx.prompt_for_new_path(&dir, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(p))) = rx.await else { return false };
            this.update_in(cx, |this, window, cx| {
                if mode == SaveMode::SaveCopy {
                    match editor.read(cx).buf(cx).save_copy(&p) {
                        Ok(()) => {
                            this.notify_message(format!("Saved a copy to {}", p.display()), cx);
                            true
                        }
                        Err(e) => {
                            this.notify_message(format!("Save failed: {e}"), cx);
                            false
                        }
                    }
                } else {
                    let ok = this.write_editor(&editor, &p, cx);
                    if ok {
                        this.refresh_menus(window, cx);
                    }
                    ok
                }
            })
            .unwrap_or(false)
        })
    }

    fn write_editor(&mut self, editor: &Entity<Editor>, path: &Path, cx: &mut Context<Self>) -> bool {
        let s = cx.global::<Settings>().clone();
        if s.trim_trailing_on_save || s.ensure_final_newline {
            editor.update(cx, |e, cx| {
                let b = e.buf(cx);
                let mut edits = Vec::new();
                if s.trim_trailing_on_save {
                    for l in 0..b.len_lines() {
                        let t = b.line_text(l);
                        let trimmed = t.trim_end_matches([' ', '\t']).chars().count();
                        let n = t.chars().count();
                        if trimmed < n {
                            let st = b.line_to_char(l);
                            edits.push((st + trimmed..st + n, String::new()));
                        }
                    }
                }
                if s.ensure_final_newline && b.len_chars() > 0 && b.line_len(b.len_lines() - 1) > 0 {
                    let end = b.len_chars();
                    edits.push((end..end, b.eol.as_str().to_string()));
                }
                if !edits.is_empty() {
                    e.apply_edits_mapping(edits, cx);
                }
            });
        }
        let buffer = editor.read(cx).buffer.clone();
        let result = buffer.update(cx, |b, cx| b.save_to(path, cx));
        match result {
            Ok(unmappable) => {
                if unmappable {
                    self.notify_message("Some characters could not be represented in the file's encoding and were replaced.", cx);
                }
                let max = s.max_recent_files;
                self.state.push_recent(path, max);
                self.state.save();
                if let Some(bk) = buffer.update(cx, |b, _| b.backup_file.take()) {
                    let _ = std::fs::remove_file(bk);
                }
                // Saving keymap.json applies the shortcuts immediately.
                let keymap_path = crate::keymap::user_keymap_path();
                if std::fs::canonicalize(&keymap_path).ok().as_deref() == buffer.read(cx).file.as_deref() {
                    let (n, errors) = crate::keymap::reload_keymap(cx);
                    if errors.is_empty() {
                        self.notify_message(format!("Keyboard shortcuts reloaded ({n} custom)"), cx);
                    } else {
                        self.notify_message(format!("keymap.json: {}", errors.join("; ")), cx);
                    }
                    self.menus_signature.clear();
                }
                cx.notify();
                true
            }
            Err(e) => {
                self.notify_message(format!("Cannot save {}: {e}", path.display()), cx);
                false
            }
        }
    }

    // ------------------------------------------------------------------ closing

    pub fn close_tab(&mut self, _: &CloseTab, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            self.close_editors(vec![e], window, cx).detach();
        }
    }

    pub fn close_tab_at(&mut self, pane: usize, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.panes.get(pane).and_then(|p| p.items.get(index)).cloned() {
            self.close_editors(vec![e], window, cx).detach();
        }
    }

    fn needs_save_prompt(&self, editor: &Entity<Editor>, closing: &[Entity<Editor>], cx: &gpui::App) -> bool {
        let e = editor.read(cx);
        let b = e.buf(cx);
        if !b.is_dirty() {
            return false;
        }
        if b.file.is_none() && b.len_chars() == 0 {
            return false;
        }
        // Another view of the same buffer stays open?
        let bid = e.buffer.entity_id();
        !self.all_editors().iter().any(|o| o != editor && !closing.contains(o) && o.read(cx).buffer.entity_id() == bid)
    }

    /// Close editors, asking to save modified documents. Resolves to false if cancelled.
    pub fn close_editors(&mut self, editors: Vec<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        cx.spawn_in(window, async move |this, cx| {
            for (idx, editor) in editors.iter().enumerate() {
                let rest = editors[idx + 1..].to_vec();
                let Ok(needs) = this.update(cx, |this, cx| this.needs_save_prompt(editor, &rest, cx)) else { return false };
                if needs {
                    let Ok(answer) = this.update_in(cx, |this, window, cx| {
                        if let Some((pi, i)) = this.find_editor_location(editor) {
                            this.activate(pi, i, window, cx);
                        }
                        let name = editor.read(cx).buf(cx).full_name();
                        window.prompt(PromptLevel::Warning, &format!("Do you want to save the changes to \"{name}\"?"), Some("Your changes will be lost if you don't save them."), &["Save", "Don't Save", "Cancel"], cx)
                    }) else {
                        return false;
                    };
                    match answer.await {
                        Ok(0) => {
                            let Ok(task) = this.update_in(cx, |this, window, cx| this.save_editor(editor.clone(), SaveMode::Save, window, cx)) else {
                                return false;
                            };
                            if !task.await {
                                return false;
                            }
                        }
                        Ok(1) => {}
                        _ => return false,
                    }
                }
                let _ = this.update_in(cx, |this, window, cx| {
                    if let Some((pi, i)) = this.find_editor_location(editor) {
                        this.remove_editor(pi, i, window, cx);
                    }
                });
            }
            true
        })
    }

    fn close_filtered(&mut self, window: &mut Window, cx: &mut Context<Self>, keep: impl Fn(&Workspace, usize, usize, &Entity<Editor>, &gpui::App) -> bool) {
        let mut list = Vec::new();
        for (pi, p) in self.panes.iter().enumerate() {
            for (i, e) in p.items.iter().enumerate() {
                if !keep(self, pi, i, e, cx) {
                    list.push(e.clone());
                }
            }
        }
        self.close_editors(list, window, cx).detach();
    }

    pub fn close_all(&mut self, _: &CloseAll, window: &mut Window, cx: &mut Context<Self>) {
        self.close_filtered(window, cx, |_, _, _, _, _| false);
    }
    pub fn close_all_but_active(&mut self, _: &CloseAllButActive, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.active_editor();
        self.close_filtered(window, cx, move |_, _, _, e, _| Some(e) == active.as_ref());
    }
    pub fn close_all_but_pinned(&mut self, _: &CloseAllButPinned, window: &mut Window, cx: &mut Context<Self>) {
        self.close_filtered(window, cx, |ws, _, _, e, _| ws.pinned.contains(&e.entity_id()));
    }
    pub fn close_all_to_left(&mut self, _: &CloseAllToLeft, window: &mut Window, cx: &mut Context<Self>) {
        let (ap, ai) = (self.active_pane, self.panes[self.active_pane].active);
        self.close_filtered(window, cx, move |_, pi, i, _, _| pi != ap || i >= ai);
    }
    pub fn close_all_to_right(&mut self, _: &CloseAllToRight, window: &mut Window, cx: &mut Context<Self>) {
        let (ap, ai) = (self.active_pane, self.panes[self.active_pane].active);
        self.close_filtered(window, cx, move |_, pi, i, _, _| pi != ap || i <= ai);
    }
    pub fn close_all_unchanged(&mut self, _: &CloseAllUnchanged, window: &mut Window, cx: &mut Context<Self>) {
        self.close_filtered(window, cx, |_, _, _, e, cx| e.read(cx).buf(cx).is_dirty());
    }

    // ------------------------------------------------------------------ misc file commands

    pub fn reload_from_disk(&mut self, _: &ReloadFromDisk, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let (dirty, name) = {
            let b = e.read(cx).buf(cx);
            if b.file.is_none() {
                return;
            }
            (b.is_dirty(), b.full_name())
        };
        let buffer = e.read(cx).buffer.clone();
        if dirty {
            let answer = window.prompt(
                PromptLevel::Warning,
                &format!("Reload \"{name}\" from disk?"),
                Some("Your unsaved changes will be lost."),
                &["Reload", "Cancel"],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                if let Ok(0) = answer.await {
                    let _ = this.update(cx, |this, cx| {
                        if let Err(err) = buffer.update(cx, |b, cx| b.reload_from_disk(cx)) {
                            this.notify_message(format!("Reload failed: {err}"), cx);
                        }
                    });
                }
            })
            .detach();
        } else if let Err(err) = buffer.update(cx, |b, cx| b.reload_from_disk(cx)) {
            self.notify_message(format!("Reload failed: {err}"), cx);
        }
    }

    pub fn rename_file(&mut self, _: &RenameFile, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let path = e.read(cx).buf(cx).file.clone();
        let Some(path) = path else {
            // Untitled: rename the tab.
            let view = cx.new(|cx| super::dialogs::InputDialog::new(self.weak_self.clone(), "Rename", "New name:", &e.read(cx).buf(cx).untitled_name.clone(), super::dialogs::InputPurpose::RenameUntitled, window, cx));
            self.open_modal(view, window, cx);
            return;
        };
        let dir = path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(new_path))) = rx.await else { return };
            let _ = this.update(cx, |this, cx| {
                match std::fs::rename(&path, &new_path) {
                    Ok(()) => {
                        let buffer = e.read(cx).buffer.clone();
                        buffer.update(cx, |b, cx| {
                            b.file = Some(new_path.clone());
                            b.disk_mtime = std::fs::metadata(&new_path).ok().and_then(|m| m.modified().ok());
                            cx.emit(crate::buffer::BufferEvent::MetaChanged);
                            cx.notify();
                        });
                        cx.notify();
                    }
                    Err(err) => this.notify_message(format!("Rename failed: {err}"), cx),
                }
            });
        })
        .detach();
    }

    pub fn delete_from_disk(&mut self, _: &DeleteFromDisk, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let Some(path) = e.read(cx).buf(cx).file.clone() else { return };
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Move \"{}\" to the Trash?", path.display()),
            None,
            &["Move to Trash", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(0) = answer.await {
                let _ = this.update_in(cx, |this, window, cx| match trash::delete(&path) {
                    Ok(()) => {
                        e.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.mark_saved_now()));
                        if let Some((pi, i)) = this.find_editor_location(&e) {
                            this.remove_editor(pi, i, window, cx);
                        }
                    }
                    Err(err) => this.notify_message(format!("Delete failed: {err}"), cx),
                });
            }
        })
        .detach();
    }

    pub fn reopen_closed_tab(&mut self, _: &ReopenClosedTab, window: &mut Window, cx: &mut Context<Self>) {
        while let Some(p) = self.closed_files.pop() {
            if p.exists() && self.editor_for_path(&p, cx).is_none() {
                self.open_paths(vec![p], window, cx);
                return;
            }
        }
    }

    pub fn open_recent(&mut self, a: &OpenRecent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.state.recent_files.get(a.index).cloned() {
            let p = PathBuf::from(p);
            if p.exists() {
                self.open_paths(vec![p], window, cx);
            } else {
                self.notify_message(format!("{} no longer exists", p.display()), cx);
                self.state.recent_files.remove(a.index);
                self.state.save();
                self.refresh_menus(window, cx);
            }
        }
    }

    pub fn open_all_recent(&mut self, _: &OpenAllRecentFiles, window: &mut Window, cx: &mut Context<Self>) {
        let paths: Vec<PathBuf> = self.state.recent_files.iter().map(PathBuf::from).filter(|p| p.exists()).collect();
        self.open_paths(paths, window, cx);
    }

    pub fn clear_recent(&mut self, _: &ClearRecentFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.state.recent_files.clear();
        self.state.save();
        self.refresh_menus(window, cx);
    }

    fn active_path(&self, cx: &gpui::App) -> Option<PathBuf> {
        self.active_editor().and_then(|e| e.read(cx).buf(cx).file.clone())
    }

    pub fn open_containing_folder(&mut self, _: &OpenContainingFolder, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.active_path(cx) {
            cx.reveal_path(&p);
        }
    }

    pub fn open_in_default_viewer(&mut self, _: &OpenInDefaultViewer, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.active_path(cx) {
            cx.open_with_system(&p);
        }
    }

    pub fn open_terminal_here(&mut self, _: &OpenTerminalHere, _: &mut Window, cx: &mut Context<Self>) {
        let dir = self.active_path(cx).and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap_or_else(|| self.default_dir(cx));
        let _ = std::process::Command::new("open").arg("-a").arg("Terminal").arg(dir).spawn();
    }

    pub fn copy_full_path(&mut self, _: &CopyFullPath, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            let s = e.read(cx).buf(cx).full_name();
            cx.write_to_clipboard(ClipboardItem::new_string(s));
        }
    }

    pub fn copy_file_name(&mut self, _: &CopyFileName, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            let s = e.read(cx).buf(cx).display_name();
            cx.write_to_clipboard(ClipboardItem::new_string(s));
        }
    }

    pub fn copy_dir_path(&mut self, _: &CopyDirPath, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.active_path(cx).and_then(|p| p.parent().map(|d| d.to_path_buf())) {
            cx.write_to_clipboard(ClipboardItem::new_string(p.to_string_lossy().to_string()));
        }
    }

    pub fn copy_all_names(&mut self, _: &CopyAllNames, _: &mut Window, cx: &mut Context<Self>) {
        let names: Vec<String> = self.unique_buffers(cx).iter().map(|(_, b)| b.read(cx).display_name()).collect();
        cx.write_to_clipboard(ClipboardItem::new_string(names.join("\n")));
    }

    pub fn copy_all_paths(&mut self, _: &CopyAllPaths, _: &mut Window, cx: &mut Context<Self>) {
        let names: Vec<String> = self.unique_buffers(cx).iter().map(|(_, b)| b.read(cx).full_name()).collect();
        cx.write_to_clipboard(ClipboardItem::new_string(names.join("\n")));
    }

    pub fn toggle_read_only(&mut self, _: &ToggleReadOnly, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            e.update(cx, |e, cx| e.buffer.update(cx, |b, cx| {
                b.user_read_only = !b.user_read_only;
                cx.notify();
            }));
            cx.notify();
        }
    }

    pub fn clear_read_only_flag(&mut self, _: &ClearReadOnlyFlag, _: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let Some(p) = self.active_path(cx) else { return };
        if let Ok(meta) = std::fs::metadata(&p) {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            match std::fs::set_permissions(&p, perms) {
                Ok(()) => {
                    e.update(cx, |e, cx| e.buffer.update(cx, |b, cx| {
                        b.file_read_only = false;
                        b.user_read_only = false;
                        cx.notify();
                    }));
                }
                Err(err) => self.notify_message(format!("Cannot change permissions: {err}"), cx),
            }
        }
        cx.notify();
    }

    pub fn print(&mut self, _: &Print, _: &mut Window, cx: &mut Context<Self>) {
        // Print through macOS: write an HTML rendition and hand it to the default browser's print flow.
        let Some(e) = self.active_editor() else { return };
        let b = e.read(cx).buf(cx);
        let name = b.display_name();
        let text = b.text();
        let escaped = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let html = format!(
            "<!doctype html><meta charset=utf-8><title>{name}</title><style>body{{font:11px Menlo,monospace;white-space:pre-wrap}}</style><body onload='window.print()'>{escaped}</body>"
        );
        let path = std::env::temp_dir().join(format!("linewright-print-{}.html", std::process::id()));
        if std::fs::write(&path, html).is_ok() {
            cx.open_with_system(&path);
        }
    }

    // ------------------------------------------------------------------ external changes

    pub(crate) fn check_files_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.is_some() || self.prompting {
            return;
        }
        let auto_reload = cx.global::<Settings>().auto_reload_unmodified;
        let detection = cx.global::<Settings>().file_change_detection;
        for (editor, buffer) in self.unique_buffers(cx) {
            let (path, mtime, dirty, monitoring, missing) = {
                let b = buffer.read(cx);
                (b.file.clone(), b.disk_mtime, b.is_dirty(), b.monitoring, b.disk_missing)
            };
            let Some(path) = path else { continue };
            let meta = std::fs::metadata(&path);
            match meta {
                Err(_) => {
                    if !missing && detection {
                        buffer.update(cx, |b, cx| {
                            b.disk_missing = true;
                            b.mark_meta_dirty();
                            cx.notify();
                        });
                        self.notify_message(format!("\"{}\" has been removed from disk. It stays open as an unsaved document.", path.display()), cx);
                    }
                }
                Ok(m) => {
                    let new_mtime = m.modified().ok();
                    if new_mtime == mtime || new_mtime.is_none() {
                        continue;
                    }
                    if !detection && !monitoring {
                        continue;
                    }
                    buffer.update(cx, |b, _| b.disk_mtime = new_mtime);
                    if monitoring || (!dirty && auto_reload) {
                        let _ = buffer.update(cx, |b, cx| b.reload_from_disk(cx));
                        if monitoring {
                            editor.update(cx, |e, cx| {
                                let end = e.buf(cx).len_chars();
                                e.set_selections(vec![Selection::caret(end)], cx);
                            });
                        }
                        continue;
                    }
                    self.prompting = true;
                    let title = format!("\"{}\" changed on disk.", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                    let msg = if dirty {
                        format!("{}\n\nReload it and discard your unsaved changes?", path.display())
                    } else {
                        format!("{}\n\nReload it?", path.display())
                    };
                    let answer = window.prompt(PromptLevel::Warning, &title, Some(&msg), &["Reload", "Keep Current"], cx);
                    cx.spawn_in(window, async move |this, cx| {
                        let a = answer.await;
                        let _ = this.update(cx, |this, cx| {
                            this.prompting = false;
                            if let Ok(0) = a {
                                let _ = buffer.update(cx, |b, cx| b.reload_from_disk(cx));
                            }
                        });
                    })
                    .detach();
                    return;
                }
            }
        }
    }

    // ------------------------------------------------------------------ backups & session

    pub(crate) fn write_backups(&mut self, cx: &mut Context<Self>) {
        if !crate::session::session_enabled() {
            return;
        }
        let mut changed = false;
        for (_, buffer) in self.unique_buffers(cx) {
            let (dirty, version, backup_version, has_backup, empty_untitled) = {
                let b = buffer.read(cx);
                (b.is_dirty(), b.version(), b.backup_version, b.backup_file.is_some(), b.file.is_none() && b.len_chars() == 0)
            };
            if dirty && !empty_untitled {
                if version != backup_version || !has_backup {
                    let (path, rope) = buffer.update(cx, |b, _| {
                        let p = b.backup_file.clone().unwrap_or_else(|| crate::session::new_backup_path(&b.display_name(), b.id));
                        b.backup_file = Some(p.clone());
                        b.backup_version = b.version();
                        (p, b.rope().clone())
                    });
                    cx.background_executor()
                        .spawn(async move {
                            if let Ok(f) = std::fs::File::create(&path) {
                                let _ = rope.write_to(std::io::BufWriter::new(f));
                            }
                        })
                        .detach();
                    changed = true;
                }
            } else if has_backup {
                if let Some(p) = buffer.update(cx, |b, _| b.backup_file.take()) {
                    let _ = std::fs::remove_file(p);
                    changed = true;
                }
            }
        }
        if changed || self.session_dirty {
            self.session_dirty = false;
            self.save_session(cx);
        }
    }

    pub fn session_snapshot(&self, cx: &gpui::App) -> Session {
        let mut panes = Vec::new();
        for p in &self.panes {
            let mut files = Vec::new();
            for e in &p.items {
                let ed = e.read(cx);
                let b = ed.buf(cx);
                let sel = ed.primary_selection();
                files.push(SessionFile {
                    path: b.file.as_ref().map(|p| p.to_string_lossy().to_string()),
                    untitled_name: if b.file.is_none() { Some(b.untitled_name.clone()) } else { None },
                    backup: b.backup_file.as_ref().map(|p| p.to_string_lossy().to_string()),
                    dirty: b.is_dirty(),
                    anchor: sel.anchor,
                    caret: sel.head,
                    scroll_x: ed.scroll_x,
                    scroll_y: ed.scroll_y,
                    language: b.syntax_name.clone(),
                    encoding: b.encoding.id(),
                    eol: b.eol.id().to_string(),
                    read_only: b.user_read_only,
                    bookmarks: b.bookmarks.iter().copied().collect(),
                    folds: ed.folds.iter().map(|(a, b)| (*a, *b)).collect(),
                    monitoring: b.monitoring,
                    pinned: self.pinned.contains(&e.entity_id()),
                });
            }
            panes.push(SessionPane { files, active: p.active });
        }
        Session {
            panes,
            active_pane: self.active_pane,
            window: self.window_bounds,
            folder_roots: self.folder.read(cx).roots.iter().map(|p| p.to_string_lossy().to_string()).collect(),
            left_panel: self.left_panel.map(|p| format!("{p:?}")),
            right_panel: self.right_panel.map(|p| format!("{p:?}")),
            split_ratio: self.split_ratio,
        }
    }

    pub fn save_session(&self, cx: &gpui::App) {
        if cx.global::<Settings>().remember_session || cx.global::<Settings>().session_snapshot {
            self.session_snapshot(cx).save();
        }
    }

    /// Restore a session. Returns true if at least one document was opened.
    pub fn restore_session(&mut self, session: &Session, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mut opened = false;
        let snapshot = cx.global::<Settings>().session_snapshot;
        for (pi, sp) in session.panes.iter().enumerate().take(2) {
            for f in &sp.files {
                let backup = f.backup.as_ref().map(PathBuf::from).filter(|p| p.exists() && snapshot);
                let path = f.path.as_ref().map(PathBuf::from);
                let id = self.alloc_buffer_id();
                let enc = TextEncoding::from_id(&f.encoding).unwrap_or_default();
                let eol = EolMode::from_id(&f.eol);
                let buffer = if let Some(bk) = &backup {
                    let Ok(text) = std::fs::read_to_string(bk) else { continue };
                    let name = f.untitled_name.clone().unwrap_or_else(|| "new 1".into());
                    let path = path.clone();
                    let bk = bk.clone();
                    let lang = f.language.clone();
                    cx.new(move |_| {
                        let mut b = Buffer::new(id, name, eol, enc);
                        b.set_text_initial(&text);
                        b.file = path.clone();
                        b.disk_mtime = path.as_ref().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok());
                        b.set_syntax(lang);
                        b.backup_file = Some(bk);
                        b.mark_dirty_forced();
                        b
                    })
                } else if let Some(p) = &path {
                    let Ok(loaded) = buffer::load_file(p) else { continue };
                    let p2 = p.clone();
                    let lang = f.language.clone();
                    cx.new(move |_| {
                        let mut b = Buffer::from_loaded(id, p2, loaded, eol);
                        if !lang.is_empty() {
                            b.set_syntax(lang);
                        }
                        b
                    })
                } else {
                    continue;
                };
                buffer.update(cx, |b, _| {
                    b.bookmarks = f.bookmarks.iter().copied().collect();
                    b.user_read_only = f.read_only;
                    b.monitoring = f.monitoring;
                });
                let editor = cx.new(|cx| Editor::new(buffer, crate::editor::EditorMode::Full, window, cx));
                self.attach_editor(&editor, window, cx);
                let (anchor, caret) = (f.anchor, f.caret);
                let folds = f.folds.clone();
                let (sx, sy) = (f.scroll_x, f.scroll_y);
                editor.update(cx, |e, cx| {
                    e.folds = folds.into_iter().collect();
                    e.set_selections(vec![Selection::new(anchor, caret)], cx);
                    e.autoscroll = None;
                    e.scroll_x = sx;
                    e.scroll_y = sy;
                });
                if f.pinned {
                    self.pinned.insert(editor.entity_id());
                }
                self.panes[pi].items.push(editor);
                opened = true;
            }
            self.panes[pi].active = sp.active.min(self.panes[pi].items.len().saturating_sub(1));
        }
        if self.panes[0].items.is_empty() && !self.panes[1].items.is_empty() {
            self.panes.swap(0, 1);
        }
        self.active_pane = if session.active_pane < 2 && !self.panes[session.active_pane].items.is_empty() { session.active_pane } else { 0 };
        for r in &session.folder_roots {
            let p = PathBuf::from(r);
            if p.is_dir() {
                self.folder.update(cx, |f, cx| f.add_root(p, cx));
            }
        }
        self.left_panel = match session.left_panel.as_deref() {
            Some("Folder") => Some(super::LeftPanel::Folder),
            Some("DocList") => Some(super::LeftPanel::DocList),
            _ => None,
        };
        self.right_panel = match session.right_panel.as_deref() {
            Some("FunctionList") => Some(super::RightPanel::FunctionList),
            Some("DocMap") => Some(super::RightPanel::DocMap),
            Some("CharPanel") => Some(super::RightPanel::CharPanel),
            Some("Clipboard") => Some(super::RightPanel::Clipboard),
            _ => None,
        };
        if session.split_ratio > 0.05 {
            self.split_ratio = session.split_ratio;
        }
        if let Some(e) = self.active_editor() {
            window.focus(&e.focus_handle(cx));
        }
        self.on_active_changed(window, cx);
        opened
    }

    pub fn load_session(&mut self, _: &LoadSession, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some("Load Session".into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(p) = paths.first() {
                    if let Some(session) = Session::load_from(p) {
                        let _ = this.update_in(cx, |this, window, cx| {
                            this.restore_session(&session, window, cx);
                            cx.notify();
                        });
                    } else {
                        let _ = this.update(cx, |this, cx| this.notify_message("Not a valid session file", cx));
                    }
                }
            }
        })
        .detach();
    }

    pub fn save_session_as(&mut self, _: &SaveSession, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.default_dir(cx);
        let rx = cx.prompt_for_new_path(&dir, Some("session.linewright-session"));
        let session = self.session_snapshot(cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(p))) = rx.await {
                session.save_to(&p);
                let _ = this.update(cx, |this, cx| this.notify_message(format!("Session saved to {}", p.display()), cx));
            }
        })
        .detach();
    }

    /// Prepare to quit. With session snapshot enabled nothing is asked;
    /// otherwise modified documents are offered for saving. Resolves to true if quitting may proceed.
    pub fn prepare_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Task<bool> {
        self.quitting = true;
        if cx.global::<Settings>().session_snapshot && crate::session::session_enabled() {
            self.write_backups(cx);
            // Make sure backups for untitled edits exist synchronously.
            for (_, buffer) in self.unique_buffers(cx) {
                let b = buffer.read(cx);
                if let Some(p) = &b.backup_file {
                    let _ = b.rope().write_to(std::io::BufWriter::new(match std::fs::File::create(p) {
                        Ok(f) => f,
                        Err(_) => continue,
                    }));
                }
            }
            self.save_session(cx);
            return Task::ready(true);
        }
        self.save_session(cx);
        let dirty: Vec<Entity<Editor>> = self.all_editors();
        let task = self.close_editors(dirty, window, cx);
        cx.spawn(async move |this, cx| {
            let ok = task.await;
            if !ok {
                let _ = this.update(cx, |this, _| this.quitting = false);
            }
            ok
        })
    }
}

