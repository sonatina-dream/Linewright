//! Menu command handlers (View, Encoding, Language, Settings, Tools, Macro, Run, Window, ?)
//! and registration of all workspace actions.

use std::path::PathBuf;

use gpui::{
    prelude::*, ClipboardItem, Context, Div, Focusable, PathPromptOptions, Point, Pixels, Stateful,
    Window,
};

use super::dialogs::{self, InfoDialog};
use super::{LeftPanel, MenuEntry, RightPanel, Workspace};
use crate::actions::*;
use crate::buffer::{Mark, Selection};
use crate::editor::{self, Editor, MacroRecorder, MacroStep};
use crate::encoding::{EolMode, TextEncoding};
use crate::session::{SavedMacro, SavedStep};
use crate::settings::{self, Settings};
use crate::tools::HashKind;

macro_rules! listen {
    ($root:expr, $cx:expr, [$($method:ident),* $(,)?]) => {
        $root $(.on_action($cx.listener(Self::$method)))*
    };
}

impl Workspace {
    pub(crate) fn register_actions(&mut self, root: Stateful<Div>, _window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let root = listen!(root, cx, [
            new_file, open_file, open_folder, reload_from_disk, save, save_as, save_copy_as, save_all,
            rename_file, close_tab, close_all, close_all_but_active, close_all_but_pinned, close_all_to_left,
            close_all_to_right, close_all_unchanged, delete_from_disk, load_session, save_session_as,
            reopen_closed_tab, open_all_recent, clear_recent, open_recent, open_containing_folder,
            open_in_default_viewer, open_terminal_here, print, quit, new_window,
            copy_full_path, copy_file_name, copy_dir_path, copy_all_names, copy_all_paths, toggle_read_only,
            clear_read_only_flag, column_editor, character_panel, clipboard_history_panel, open_selection_as_file,
            search_on_internet,
            find, replace, find_in_files, mark_dialog, find_next, find_prev, select_and_find_next,
            select_and_find_prev, find_word_next, find_word_prev, incremental_search, go_to_line,
            show_search_results, next_search_result, prev_search_result, find_chars_in_range,
            use_selection_for_find, close_find_panel, select_all_occurrences, mark_all_occurrences,
            toggle_always_on_top, toggle_full_screen, toggle_floating_note, toggle_focus_mode, view_in_browser,
            toggle_word_wrap, toggle_show_whitespace, toggle_show_eol, toggle_show_all_chars, toggle_indent_guides,
            toggle_wrap_symbol, toggle_line_numbers, toggle_bookmark_margin, toggle_fold_margin, toggle_change_history,
            toggle_current_line_highlight, toggle_edge, toggle_smart_highlight, zoom_in, zoom_out, zoom_reset,
            move_to_other_view, clone_to_other_view, focus_other_view, toggle_split_orientation, next_tab, prev_tab,
            move_tab_forward, move_tab_backward, go_to_tab_1, go_to_tab_2, go_to_tab_3, go_to_tab_4, go_to_tab_5,
            go_to_tab_6, go_to_tab_7, go_to_tab_8, go_to_last_tab, activate_tab, toggle_project_folder,
            toggle_function_list, toggle_minimap, toggle_open_documents, toggle_search_results_panel,
            toggle_sync_vertical_scroll, toggle_sync_horizontal_scroll, toggle_monitoring, show_statistics,
            toggle_toolbar, toggle_status_bar, toggle_tab_bar,
            encode_ansi, encode_utf8, encode_utf8_bom, encode_utf16_be, encode_utf16_le, convert_to_ansi,
            convert_to_utf8, convert_to_utf8_bom, convert_to_utf16_be, convert_to_utf16_le, set_character_set,
            eol_windows, eol_unix, eol_mac, set_language,
            open_preferences, open_themes, edit_keymap_file, open_config_folder, set_theme,
            toggle_auto_completion, toggle_auto_close_brackets, toggle_auto_indent, toggle_use_spaces, set_tab_size,
            hash_md5, hash_sha1, hash_sha256, hash_sha512, hash_md5_files, hash_sha256_files, hash_md5_selection,
            hash_sha1_selection, hash_sha256_selection, hash_sha512_selection,
            start_recording, stop_recording, toggle_recording, playback_macro, save_macro, run_macro_multiple,
            trim_trailing_and_save, run_saved_macro,
            run_command, launch_in_safari, launch_in_chrome, launch_in_firefox,
            compare_with_other_view, clear_compare,
            windows_list, sort_tabs_by_name, sort_tabs_by_path, about, show_third_party_licenses, show_shortcuts, open_homepage, debug_info,
            dismiss_modal, show_encoding_menu_action,
            fp_find_next, fp_find_prev, fp_count, fp_find_all_current, fp_find_all_open, fp_replace, fp_replace_all,
            fp_replace_all_open, fp_find_in_files, fp_replace_in_files, fp_mark_all, fp_clear_marks, fp_copy_marked,
            fp_browse_dir, fp_use_current_dir,
        ]);
        let mut root = root.on_action(cx.listener(|this, _: &editor::Cancel, window, cx| this.escape(window, cx)));
        // Forward editor commands (from the menu bar / toolbar) to the active document when the
        // focus is elsewhere (find panel, folder tree, ...).
        for a in editor::basic_actions().into_iter().chain(editor::doc_actions()) {
            if a.as_any().is::<editor::Cancel>() {
                continue;
            }
            let weak = self.weak_self.clone();
            root = root.on_boxed_action(&*a, move |action, window, cx| {
                let Some(ws) = weak.upgrade() else { return };
                let Some(ed) = ws.read(cx).active_editor() else { return };
                ed.update(cx, |e, cx| {
                    if !e.dispatch_doc_action(action, window, cx) {
                        e.dispatch_basic_action(action, window, cx);
                    }
                });
            });
        }
        root
    }

    fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.context_menu.take().is_some() {
        } else if self.modal.is_some() {
            self.close_modal(window, cx);
        } else if self.incremental.is_some() {
            self.close_incremental(window, cx);
        } else if self.find_visible {
            self.find_visible = false;
            self.focus_active(window, cx);
        } else if self.focus_mode || self.floating_note {
            self.focus_mode = false;
            self.floating_note = false;
        } else {
            self.focus_active(window, cx);
        }
        cx.notify();
    }

    fn dismiss_modal(&mut self, _: &DismissModal, window: &mut Window, cx: &mut Context<Self>) {
        self.close_modal(window, cx);
    }

    pub fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        let task = self.prepare_quit(window, cx);
        cx.spawn(async move |_, cx| {
            if task.await {
                let _ = cx.update(|cx| cx.quit());
            }
        })
        .detach();
    }

    fn new_window(&mut self, _: &NewWindow, _: &mut Window, cx: &mut Context<Self>) {
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).arg("--new-instance").spawn();
        } else {
            self.notify_message("Could not start a new instance", cx);
        }
    }

    // ------------------------------------------------------------------ settings toggles

    fn toggle_setting(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Settings)) {
        settings::update_settings(cx, f);
        for e in self.all_editors() {
            e.update(cx, |e, cx| {
                e.display.invalidate();
                cx.notify();
            });
        }
        cx.notify();
    }

    fn toggle_word_wrap(&mut self, _: &ToggleWordWrap, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.word_wrap = !s.word_wrap);
    }
    fn toggle_show_whitespace(&mut self, _: &ToggleShowWhitespace, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_whitespace = !s.show_whitespace);
    }
    fn toggle_show_eol(&mut self, _: &ToggleShowEol, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_eol = !s.show_eol);
    }
    fn toggle_show_all_chars(&mut self, _: &ToggleShowAllChars, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| {
            let on = !(s.show_whitespace && s.show_eol);
            s.show_whitespace = on;
            s.show_eol = on;
        });
    }
    fn toggle_indent_guides(&mut self, _: &ToggleIndentGuides, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_indent_guides = !s.show_indent_guides);
    }
    fn toggle_wrap_symbol(&mut self, _: &ToggleWrapSymbol, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_wrap_symbol = !s.show_wrap_symbol);
    }
    fn toggle_line_numbers(&mut self, _: &ToggleLineNumbers, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_line_numbers = !s.show_line_numbers);
    }
    fn toggle_bookmark_margin(&mut self, _: &ToggleBookmarkMargin, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_bookmark_margin = !s.show_bookmark_margin);
    }
    fn toggle_fold_margin(&mut self, _: &ToggleFoldMargin, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_fold_margin = !s.show_fold_margin);
    }
    fn toggle_change_history(&mut self, _: &ToggleChangeHistory, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_change_history = !s.show_change_history);
    }
    fn toggle_current_line_highlight(&mut self, _: &ToggleCurrentLineHighlight, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.highlight_current_line = !s.highlight_current_line);
    }
    fn toggle_edge(&mut self, _: &ToggleEdge, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_edge = !s.show_edge);
    }
    fn toggle_smart_highlight(&mut self, _: &ToggleSmartHighlight, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.smart_highlight = !s.smart_highlight);
    }
    fn toggle_toolbar(&mut self, _: &ToggleToolbar, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_toolbar = !s.show_toolbar);
    }
    fn toggle_status_bar(&mut self, _: &ToggleStatusBar, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_status_bar = !s.show_status_bar);
    }
    fn toggle_tab_bar(&mut self, _: &ToggleTabBar, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.show_tab_bar = !s.show_tab_bar);
    }
    fn toggle_auto_completion(&mut self, _: &ToggleAutoCompletion, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.auto_completion = !s.auto_completion);
    }
    fn toggle_auto_close_brackets(&mut self, _: &ToggleAutoCloseBrackets, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| {
            s.auto_close_brackets = !s.auto_close_brackets;
            s.auto_close_quotes = s.auto_close_brackets;
        });
    }
    fn toggle_auto_indent(&mut self, _: &ToggleAutoIndent, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.auto_indent = !s.auto_indent);
    }
    fn toggle_use_spaces(&mut self, _: &ToggleUseSpaces, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.use_spaces = !s.use_spaces);
    }
    fn set_tab_size(&mut self, a: &SetTabSize, _: &mut Window, cx: &mut Context<Self>) {
        let n = a.size.clamp(1, 16);
        self.toggle_setting(cx, |s| s.tab_size = n);
    }

    pub fn zoom_in(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.zoom = (s.zoom + 1).min(40));
    }
    pub fn zoom_out(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.zoom = (s.zoom - 1).max(-8));
    }
    fn zoom_reset(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.toggle_setting(cx, |s| s.zoom = 0);
    }

    fn toggle_always_on_top(&mut self, _: &ToggleAlwaysOnTop, window: &mut Window, cx: &mut Context<Self>) {
        self.always_on_top = !self.always_on_top;
        crate::platform::set_always_on_top(window, self.always_on_top);
        self.refresh_menus(window, cx);
        cx.notify();
    }
    fn toggle_full_screen(&mut self, _: &ToggleFullScreen, window: &mut Window, _: &mut Context<Self>) {
        window.toggle_fullscreen();
    }
    fn toggle_floating_note(&mut self, _: &ToggleFloatingNote, window: &mut Window, cx: &mut Context<Self>) {
        self.floating_note = !self.floating_note;
        self.always_on_top = self.floating_note;
        crate::platform::set_always_on_top(window, self.floating_note);
        cx.notify();
    }
    fn toggle_focus_mode(&mut self, _: &ToggleFocusMode, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_mode = !self.focus_mode;
        if self.focus_mode != window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    // ------------------------------------------------------------------ tabs & views

    fn cycle_tab(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let p = &self.panes[self.active_pane];
        let n = p.items.len() as isize;
        if n == 0 {
            return;
        }
        let i = ((p.active as isize + delta) % n + n) % n;
        self.activate(self.active_pane, i as usize, window, cx);
    }
    fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_tab(1, window, cx);
    }
    fn prev_tab(&mut self, _: &PrevTab, window: &mut Window, cx: &mut Context<Self>) {
        self.cycle_tab(-1, window, cx);
    }
    fn go_to_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i < self.panes[self.active_pane].items.len() {
            self.activate(self.active_pane, i, window, cx);
        }
    }
    fn go_to_tab_1(&mut self, _: &GoToTab1, w: &mut Window, cx: &mut Context<Self>) { self.go_to_tab(0, w, cx) }
    fn go_to_tab_2(&mut self, _: &GoToTab2, w: &mut Window, cx: &mut Context<Self>) { self.go_to_tab(1, w, cx) }
    fn go_to_tab_3(&mut self, _: &GoToTab3, w: &mut Window, cx: &mut Context<Self>) { self.go_to_tab(2, w, cx) }
    fn go_to_tab_4(&mut self, _: &GoToTab4, w: &mut Window, cx: &mut Context<Self>) { self.go_to_tab(3, w, cx) }
    fn go_to_tab_5(&mut self, _: &GoToTab5, w: &mut Window, cx: &mut Context<Self>) { self.go_to_tab(4, w, cx) }
    fn go_to_tab_6(&mut self, _: &GoToTab6, w: &mut Window, cx: &mut Context<Self>) { self.go_to_tab(5, w, cx) }
    fn go_to_tab_7(&mut self, _: &GoToTab7, w: &mut Window, cx: &mut Context<Self>) { self.go_to_tab(6, w, cx) }
    fn go_to_tab_8(&mut self, _: &GoToTab8, w: &mut Window, cx: &mut Context<Self>) { self.go_to_tab(7, w, cx) }
    fn go_to_last_tab(&mut self, _: &GoToLastTab, w: &mut Window, cx: &mut Context<Self>) {
        let n = self.panes[self.active_pane].items.len();
        if n > 0 {
            self.go_to_tab(n - 1, w, cx);
        }
    }
    fn activate_tab(&mut self, a: &ActivateTab, w: &mut Window, cx: &mut Context<Self>) {
        self.activate(a.pane, a.index, w, cx);
    }

    pub fn move_tab(&mut self, from_pane: usize, from: usize, to_pane: usize, to: usize, window: &mut Window, cx: &mut Context<Self>) {
        if from >= self.panes[from_pane].items.len() {
            return;
        }
        if from_pane == to_pane {
            let p = &mut self.panes[from_pane];
            let e = p.items.remove(from);
            let to = if to > from { to - 1 } else { to }.min(p.items.len());
            p.items.insert(to, e);
            p.active = to;
            self.active_pane = to_pane;
            self.on_active_changed(window, cx);
        } else {
            let e = self.panes[from_pane].items.remove(from);
            let p = &mut self.panes[from_pane];
            if p.active >= p.items.len() && !p.items.is_empty() {
                p.active = p.items.len() - 1;
            }
            let to = to.min(self.panes[to_pane].items.len());
            self.panes[to_pane].items.insert(to, e.clone());
            self.panes[to_pane].active = to;
            self.active_pane = to_pane;
            if self.panes[0].items.is_empty() {
                self.panes.swap(0, 1);
                self.active_pane = 0;
            }
            window.focus(&e.focus_handle(cx));
            self.on_active_changed(window, cx);
        }
    }

    fn move_tab_forward(&mut self, _: &MoveTabForward, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.active_pane;
        let i = self.panes[p].active;
        if i + 1 < self.panes[p].items.len() {
            self.move_tab(p, i, p, i + 2, window, cx);
        }
    }
    fn move_tab_backward(&mut self, _: &MoveTabBackward, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.active_pane;
        let i = self.panes[p].active;
        if i > 0 {
            self.move_tab(p, i, p, i - 1, window, cx);
        }
    }

    fn move_to_other_view(&mut self, _: &MoveToOtherView, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.active_pane;
        if self.panes[p].items.len() <= 1 && p == 0 && !self.is_split() {
            // Moving the only document: create the split by moving it; main gets a new doc.
        }
        let i = self.panes[p].active;
        let other = 1 - p;
        let end = self.panes[other].items.len();
        if self.panes[p].items.len() == 1 && p == 0 {
            // Keep the main view non-empty.
            self.move_tab(p, i, other, end, window, cx);
            if self.panes[0].items.is_empty() {
                let buffer = self.new_buffer_entity(cx);
                self.add_buffer(buffer, 0, false, window, cx);
            }
            return;
        }
        self.move_tab(p, i, other, end, window, cx);
    }

    fn clone_to_other_view(&mut self, _: &CloneToOtherView, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let buffer = e.read(cx).buffer.clone();
        let other = self.other_pane();
        // Already cloned? Just activate it.
        if let Some(i) = self.panes[other].items.iter().position(|o| o.read(cx).buffer == buffer) {
            self.activate(other, i, window, cx);
            return;
        }
        let sel = e.read(cx).selections.clone();
        let new = self.add_buffer(buffer, other, true, window, cx);
        new.update(cx, |n, cx| n.set_selections(sel, cx));
    }

    fn focus_other_view(&mut self, _: &FocusOtherView, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_split() {
            let other = self.other_pane();
            let i = self.panes[other].active;
            self.activate(other, i, window, cx);
        }
    }

    fn toggle_split_orientation(&mut self, _: &ToggleSplitOrientation, _: &mut Window, cx: &mut Context<Self>) {
        self.split_vertical = !self.split_vertical;
        let v = self.split_vertical;
        settings::update_settings(cx, |s| s.split_vertical = v);
        cx.notify();
    }

    fn sort_tabs_by_name(&mut self, _: &SortTabsByName, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.active_pane;
        let active = self.panes[p].active_item().cloned();
        let mut items = std::mem::take(&mut self.panes[p].items);
        items.sort_by_key(|e| e.read(cx).buf(cx).display_name().to_lowercase());
        self.panes[p].active = active.and_then(|a| items.iter().position(|e| *e == a)).unwrap_or(0);
        self.panes[p].items = items;
        self.on_active_changed(window, cx);
    }

    fn sort_tabs_by_path(&mut self, _: &SortTabsByPath, window: &mut Window, cx: &mut Context<Self>) {
        let p = self.active_pane;
        let active = self.panes[p].active_item().cloned();
        let mut items = std::mem::take(&mut self.panes[p].items);
        items.sort_by_key(|e| e.read(cx).buf(cx).full_name().to_lowercase());
        self.panes[p].active = active.and_then(|a| items.iter().position(|e| *e == a)).unwrap_or(0);
        self.panes[p].items = items;
        self.on_active_changed(window, cx);
    }

    // ------------------------------------------------------------------ panels

    fn toggle_left(&mut self, panel: LeftPanel, window: &mut Window, cx: &mut Context<Self>) {
        self.left_panel = if self.left_panel == Some(panel) { None } else { Some(panel) };
        if self.left_panel == Some(LeftPanel::Folder) && self.folder.read(cx).roots.is_empty() {
            self.open_folder(&OpenFolder, window, cx);
        }
        self.session_dirty = true;
        self.refresh_menus(window, cx);
        cx.notify();
    }

    fn toggle_right(&mut self, panel: RightPanel, window: &mut Window, cx: &mut Context<Self>) {
        self.right_panel = if self.right_panel == Some(panel) { None } else { Some(panel) };
        if self.right_panel == Some(RightPanel::FunctionList) {
            self.function_list.update(cx, |f, cx| f.schedule_refresh(cx));
        }
        self.session_dirty = true;
        self.refresh_menus(window, cx);
        cx.notify();
    }

    fn toggle_project_folder(&mut self, _: &ToggleProjectFolder, w: &mut Window, cx: &mut Context<Self>) {
        self.toggle_left(LeftPanel::Folder, w, cx);
    }
    fn toggle_open_documents(&mut self, _: &ToggleOpenDocuments, w: &mut Window, cx: &mut Context<Self>) {
        self.toggle_left(LeftPanel::DocList, w, cx);
    }
    fn toggle_function_list(&mut self, _: &ToggleFunctionList, w: &mut Window, cx: &mut Context<Self>) {
        self.toggle_right(RightPanel::FunctionList, w, cx);
    }
    fn toggle_minimap(&mut self, _: &ToggleMinimap, w: &mut Window, cx: &mut Context<Self>) {
        self.toggle_right(RightPanel::DocMap, w, cx);
    }
    fn character_panel(&mut self, _: &CharacterPanel, w: &mut Window, cx: &mut Context<Self>) {
        self.toggle_right(RightPanel::CharPanel, w, cx);
    }
    fn clipboard_history_panel(&mut self, _: &ClipboardHistoryPanel, w: &mut Window, cx: &mut Context<Self>) {
        self.toggle_right(RightPanel::Clipboard, w, cx);
    }
    fn toggle_search_results_panel(&mut self, _: &ToggleSearchResultsPanel, w: &mut Window, cx: &mut Context<Self>) {
        self.results_visible = !self.results_visible;
        self.refresh_menus(w, cx);
        cx.notify();
    }

    fn toggle_sync_vertical_scroll(&mut self, _: &ToggleSyncVerticalScroll, w: &mut Window, cx: &mut Context<Self>) {
        self.sync_vertical = !self.sync_vertical;
        self.refresh_menus(w, cx);
        cx.notify();
    }
    fn toggle_sync_horizontal_scroll(&mut self, _: &ToggleSyncHorizontalScroll, w: &mut Window, cx: &mut Context<Self>) {
        self.sync_horizontal = !self.sync_horizontal;
        self.refresh_menus(w, cx);
        cx.notify();
    }

    fn toggle_monitoring(&mut self, _: &ToggleMonitoring, w: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        if e.read(cx).buf(cx).file.is_none() {
            self.notify_message("Monitoring needs a file on disk.", cx);
            return;
        }
        e.update(cx, |e, cx| {
            e.buffer.update(cx, |b, cx| {
                b.monitoring = !b.monitoring;
                b.user_read_only = b.monitoring;
                cx.notify();
            });
            let end = e.buf(cx).len_chars();
            e.set_selections(vec![Selection::caret(end)], cx);
        });
        self.refresh_menus(w, cx);
        cx.notify();
    }

    fn show_statistics(&mut self, _: &ShowStatistics, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let ed = e.read(cx);
        let b = ed.buf(cx);
        let text = b.text();
        let chars_no_eol = text.chars().filter(|c| *c != '\n' && *c != '\r').count();
        let words = crate::tools::word_count(&text);
        let bytes = crate::encoding::encode(&text, b.encoding).0.len();
        let sel_text = ed.selected_text(cx);
        let mut s = String::new();
        if let Some(p) = &b.file {
            s.push_str(&format!("Full file path: {}\n", p.display()));
            if let Ok(m) = std::fs::metadata(p) {
                let fmt = |t: std::time::SystemTime| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d %H:%M:%S").to_string();
                if let Ok(c) = m.created() {
                    s.push_str(&format!("Created: {}\n", fmt(c)));
                }
                if let Ok(md) = m.modified() {
                    s.push_str(&format!("Modified: {}\n", fmt(md)));
                }
            }
            s.push('\n');
        }
        s.push_str(&format!("Characters (without line endings): {}\n", super::chrome::fmt_num(chars_no_eol)));
        s.push_str(&format!("Words: {}\n", super::chrome::fmt_num(words)));
        s.push_str(&format!("Lines: {}\n", super::chrome::fmt_num(b.len_lines())));
        s.push_str(&format!("Document length: {} bytes ({})\n", super::chrome::fmt_num(bytes), b.encoding.label()));
        s.push_str(&format!(
            "{} selected characters ({} bytes) in {} ranges",
            super::chrome::fmt_num(sel_text.chars().count()),
            super::chrome::fmt_num(sel_text.len()),
            ed.selections.iter().filter(|s| !s.is_empty()).count()
        ));
        let view = cx.new(|cx| InfoDialog::new("Document Statistics", &s, false, window, cx));
        self.open_modal(view, window, cx);
    }

    // ------------------------------------------------------------------ encoding / eol / language

    fn with_active_buffer(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut crate::buffer::Buffer, &mut Context<crate::buffer::Buffer>)) {
        if let Some(e) = self.active_editor() {
            let b = e.read(cx).buffer.clone();
            b.update(cx, f);
            cx.notify();
        }
    }

    fn encode_in(&mut self, enc: TextEncoding, window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(cx, |b, cx| b.reinterpret_encoding(enc, cx));
        self.refresh_menus(window, cx);
    }
    fn convert_to(&mut self, enc: TextEncoding, window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(cx, |b, cx| b.convert_encoding(enc, cx));
        self.refresh_menus(window, cx);
    }
    fn encode_ansi(&mut self, _: &EncodeAnsi, w: &mut Window, cx: &mut Context<Self>) { self.encode_in(TextEncoding::ansi(), w, cx) }
    fn encode_utf8(&mut self, _: &EncodeUtf8, w: &mut Window, cx: &mut Context<Self>) { self.encode_in(TextEncoding::Utf8, w, cx) }
    fn encode_utf8_bom(&mut self, _: &EncodeUtf8Bom, w: &mut Window, cx: &mut Context<Self>) { self.encode_in(TextEncoding::Utf8Bom, w, cx) }
    fn encode_utf16_be(&mut self, _: &EncodeUtf16Be, w: &mut Window, cx: &mut Context<Self>) { self.encode_in(TextEncoding::Utf16BeBom, w, cx) }
    fn encode_utf16_le(&mut self, _: &EncodeUtf16Le, w: &mut Window, cx: &mut Context<Self>) { self.encode_in(TextEncoding::Utf16LeBom, w, cx) }
    fn convert_to_ansi(&mut self, _: &ConvertToAnsi, w: &mut Window, cx: &mut Context<Self>) { self.convert_to(TextEncoding::ansi(), w, cx) }
    fn convert_to_utf8(&mut self, _: &ConvertToUtf8, w: &mut Window, cx: &mut Context<Self>) { self.convert_to(TextEncoding::Utf8, w, cx) }
    fn convert_to_utf8_bom(&mut self, _: &ConvertToUtf8Bom, w: &mut Window, cx: &mut Context<Self>) { self.convert_to(TextEncoding::Utf8Bom, w, cx) }
    fn convert_to_utf16_be(&mut self, _: &ConvertToUtf16Be, w: &mut Window, cx: &mut Context<Self>) { self.convert_to(TextEncoding::Utf16BeBom, w, cx) }
    fn convert_to_utf16_le(&mut self, _: &ConvertToUtf16Le, w: &mut Window, cx: &mut Context<Self>) { self.convert_to(TextEncoding::Utf16LeBom, w, cx) }
    fn set_character_set(&mut self, a: &SetCharacterSet, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(enc) = TextEncoding::from_id(&a.name) {
            self.encode_in(enc, w, cx);
        }
    }

    fn set_eol(&mut self, eol: EolMode, window: &mut Window, cx: &mut Context<Self>) {
        self.with_active_buffer(cx, |b, cx| b.convert_eol(eol, cx));
        self.refresh_menus(window, cx);
    }
    fn eol_windows(&mut self, _: &EolWindows, w: &mut Window, cx: &mut Context<Self>) { self.set_eol(EolMode::Crlf, w, cx) }
    fn eol_unix(&mut self, _: &EolUnix, w: &mut Window, cx: &mut Context<Self>) { self.set_eol(EolMode::Lf, w, cx) }
    fn eol_mac(&mut self, _: &EolMac, w: &mut Window, cx: &mut Context<Self>) { self.set_eol(EolMode::Cr, w, cx) }

    fn set_language(&mut self, a: &SetLanguage, window: &mut Window, cx: &mut Context<Self>) {
        let name = a.name.clone();
        self.with_active_buffer(cx, move |b, cx| {
            b.set_syntax(name);
            cx.emit(crate::buffer::BufferEvent::MetaChanged);
        });
        if let Some(e) = self.active_editor() {
            e.update(cx, |_, cx| cx.notify());
        }
        self.function_list.update(cx, |f, cx| f.schedule_refresh(cx));
        self.refresh_menus(window, cx);
    }

    pub fn show_language_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let current = self.active_editor().map(|e| e.read(cx).buf(cx).syntax_name.clone()).unwrap_or_default();
        let mut entries = vec![MenuEntry::action("Plain Text", SetLanguage { name: crate::syntax::PLAIN_TEXT.into() }).checked(current == crate::syntax::PLAIN_TEXT)];
        entries.push(MenuEntry::separator());
        for n in crate::syntax::language_names() {
            if n == crate::syntax::PLAIN_TEXT {
                continue;
            }
            let checked = n == current;
            entries.push(MenuEntry::action(n.clone(), SetLanguage { name: n }).checked(checked));
        }
        self.show_context_menu(pos - gpui::point(gpui::px(0.0), gpui::px(300.0)), entries, cx);
    }

    pub fn show_eol_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let cur = self.active_editor().map(|e| e.read(cx).buf(cx).eol).unwrap_or_default();
        let entries = vec![
            MenuEntry::action("CRLF (Windows)", EolWindows).checked(cur == EolMode::Crlf),
            MenuEntry::action("LF (Unix and macOS)", EolUnix).checked(cur == EolMode::Lf),
            MenuEntry::action("CR (classic Mac OS)", EolMac).checked(cur == EolMode::Cr),
        ];
        self.show_context_menu(pos - gpui::point(gpui::px(0.0), gpui::px(80.0)), entries, cx);
    }

    pub fn show_encoding_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let cur = self.active_editor().map(|e| e.read(cx).buf(cx).encoding).unwrap_or_default();
        let entries = vec![
            MenuEntry::action("Encode in ANSI", EncodeAnsi).checked(cur == TextEncoding::ansi()),
            MenuEntry::action("Encode in UTF-8", EncodeUtf8).checked(cur == TextEncoding::Utf8),
            MenuEntry::action("Encode in UTF-8-BOM", EncodeUtf8Bom).checked(cur == TextEncoding::Utf8Bom),
            MenuEntry::action("Encode in UTF-16 BE BOM", EncodeUtf16Be).checked(cur == TextEncoding::Utf16BeBom),
            MenuEntry::action("Encode in UTF-16 LE BOM", EncodeUtf16Le).checked(cur == TextEncoding::Utf16LeBom),
            MenuEntry::separator(),
            MenuEntry::action("Convert to ANSI", ConvertToAnsi),
            MenuEntry::action("Convert to UTF-8", ConvertToUtf8),
            MenuEntry::action("Convert to UTF-8-BOM", ConvertToUtf8Bom),
            MenuEntry::action("Convert to UTF-16 BE BOM", ConvertToUtf16Be),
            MenuEntry::action("Convert to UTF-16 LE BOM", ConvertToUtf16Le),
        ];
        self.show_context_menu(pos - gpui::point(gpui::px(0.0), gpui::px(260.0)), entries, cx);
    }

    fn show_encoding_menu_action(&mut self, _: &ShowEncodingMenu, window: &mut Window, cx: &mut Context<Self>) {
        let pos = window.mouse_position();
        self.show_encoding_menu(pos, cx);
    }

    // ------------------------------------------------------------------ settings dialogs

    fn open_preferences(&mut self, _: &OpenPreferences, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| dialogs::PreferencesDialog::new(window, cx));
        self.open_modal(view, window, cx);
    }

    fn open_themes(&mut self, _: &OpenThemes, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| dialogs::StyleDialog::new(window, cx));
        self.open_modal(view, window, cx);
    }

    fn set_theme(&mut self, a: &SetTheme, window: &mut Window, cx: &mut Context<Self>) {
        let name = a.name.clone();
        settings::update_settings(cx, |s| s.theme = name.clone());
        crate::theme::set_theme(cx, &name);
        for e in self.all_editors() {
            e.update(cx, |_, cx| cx.notify());
        }
        self.refresh_menus(window, cx);
        cx.notify();
    }

    fn edit_keymap_file(&mut self, _: &EditKeymapFile, window: &mut Window, cx: &mut Context<Self>) {
        let path = crate::keymap::user_keymap_path();
        if !path.exists() {
            let _ = std::fs::write(&path, crate::keymap::USER_KEYMAP_TEMPLATE);
        }
        self.open_paths(vec![path], window, cx);
        self.notify_message("Edit keymap.json and save it — changes apply immediately.", cx);
    }

    fn open_config_folder(&mut self, _: &OpenConfigFolder, _: &mut Window, cx: &mut Context<Self>) {
        cx.reveal_path(&settings::config_dir().join("config.json"));
    }

    fn show_shortcuts(&mut self, _: &ShowShortcuts, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| dialogs::ShortcutsDialog::new(window, cx));
        self.open_modal(view, window, cx);
    }

    // ------------------------------------------------------------------ tools: hashes

    fn hash_dialog(&mut self, kind: HashKind, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| dialogs::HashDialog::new(kind, window, cx));
        self.open_modal(view, window, cx);
    }
    fn hash_md5(&mut self, _: &HashMd5, w: &mut Window, cx: &mut Context<Self>) { self.hash_dialog(HashKind::Md5, w, cx) }
    fn hash_sha1(&mut self, _: &HashSha1, w: &mut Window, cx: &mut Context<Self>) { self.hash_dialog(HashKind::Sha1, w, cx) }
    fn hash_sha256(&mut self, _: &HashSha256, w: &mut Window, cx: &mut Context<Self>) { self.hash_dialog(HashKind::Sha256, w, cx) }
    fn hash_sha512(&mut self, _: &HashSha512, w: &mut Window, cx: &mut Context<Self>) { self.hash_dialog(HashKind::Sha512, w, cx) }

    fn hash_files(&mut self, kind: HashKind, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Choose files".into()) });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            let out = cx
                .background_executor()
                .spawn(async move {
                    paths
                        .iter()
                        .map(|p| match std::fs::read(p) {
                            Ok(bytes) => format!("{}  {}", crate::tools::hash_hex(&bytes, kind), p.display()),
                            Err(e) => format!("error: {e}  {}", p.display()),
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let view = cx.new(|cx| InfoDialog::new(&format!("{} from files", kind.label()), &out, true, window, cx));
                this.open_modal(view, window, cx);
            });
        })
        .detach();
    }
    fn hash_md5_files(&mut self, _: &HashMd5Files, w: &mut Window, cx: &mut Context<Self>) { self.hash_files(HashKind::Md5, w, cx) }
    fn hash_sha256_files(&mut self, _: &HashSha256Files, w: &mut Window, cx: &mut Context<Self>) { self.hash_files(HashKind::Sha256, w, cx) }

    fn hash_selection(&mut self, kind: HashKind, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let text = e.read(cx).selected_text(cx);
        if text.is_empty() {
            self.notify_message("Select some text first.", cx);
            return;
        }
        let h = crate::tools::hash_hex(text.as_bytes(), kind);
        cx.write_to_clipboard(ClipboardItem::new_string(h.clone()));
        self.notify_message(format!("{} copied to clipboard: {h}", kind.label()), cx);
    }
    fn hash_md5_selection(&mut self, _: &HashMd5Selection, _: &mut Window, cx: &mut Context<Self>) { self.hash_selection(HashKind::Md5, cx) }
    fn hash_sha1_selection(&mut self, _: &HashSha1Selection, _: &mut Window, cx: &mut Context<Self>) { self.hash_selection(HashKind::Sha1, cx) }
    fn hash_sha256_selection(&mut self, _: &HashSha256Selection, _: &mut Window, cx: &mut Context<Self>) { self.hash_selection(HashKind::Sha256, cx) }
    fn hash_sha512_selection(&mut self, _: &HashSha512Selection, _: &mut Window, cx: &mut Context<Self>) { self.hash_selection(HashKind::Sha512, cx) }

    // ------------------------------------------------------------------ macros

    fn ensure_recorder(cx: &mut Context<Self>) {
        if cx.try_global::<MacroRecorder>().is_none() {
            cx.set_global(MacroRecorder::default());
        }
    }

    pub fn start_recording(&mut self, _: &StartRecording, window: &mut Window, cx: &mut Context<Self>) {
        Self::ensure_recorder(cx);
        let r = cx.global_mut::<MacroRecorder>();
        r.recording = true;
        r.steps.clear();
        self.refresh_menus(window, cx);
        self.notify_message("Macro recording started", cx);
        cx.notify();
    }

    pub fn stop_recording(&mut self, _: &StopRecording, window: &mut Window, cx: &mut Context<Self>) {
        Self::ensure_recorder(cx);
        let r = cx.global_mut::<MacroRecorder>();
        if r.recording {
            r.recording = false;
            r.last = std::mem::take(&mut r.steps);
            let n = r.last.len();
            self.notify_message(format!("Macro recorded ({n} steps)"), cx);
        }
        self.refresh_menus(window, cx);
        cx.notify();
    }

    fn toggle_recording(&mut self, _: &ToggleRecording, window: &mut Window, cx: &mut Context<Self>) {
        let recording = cx.try_global::<MacroRecorder>().map(|r| r.recording).unwrap_or(false);
        if recording {
            self.stop_recording(&StopRecording, window, cx);
        } else {
            self.start_recording(&StartRecording, window, cx);
        }
    }

    /// Run macro steps on the active editor. `times == 0` means "until end of file".
    pub fn run_macro(&mut self, steps: &[MacroStep], times: usize, window: &mut Window, cx: &mut Context<Self>) {
        if steps.is_empty() {
            self.notify_message("No macro recorded.", cx);
            return;
        }
        let Some(editor) = self.active_editor() else { return };
        let until_eof = times == 0;
        let max = if until_eof { 1_000_000 } else { times };
        for _ in 0..max {
            let before = editor.read(cx).caret();
            for step in steps {
                match step {
                    MacroStep::Insert(s) => editor.update(cx, |e, cx| e.insert_text(s, window, cx)),
                    MacroStep::Action(a) => {
                        let handled = editor.update(cx, |e, cx| e.dispatch_doc_action(&**a, window, cx) || e.dispatch_basic_action(&**a, window, cx));
                        if !handled {
                            self.dispatch_recorded_workspace_action(&**a, window, cx);
                        }
                    }
                }
            }
            if until_eof {
                let (after, len) = {
                    let e = editor.read(cx);
                    (e.caret(), e.buf(cx).len_chars())
                };
                if after >= len || after <= before {
                    break;
                }
            }
        }
    }

    fn dispatch_recorded_workspace_action(&mut self, a: &dyn gpui::Action, window: &mut Window, cx: &mut Context<Self>) {
        let any = a.as_any();
        if any.is::<FindNext>() {
            self.find_next(&FindNext, window, cx);
        } else if any.is::<FindPrev>() {
            self.find_prev(&FindPrev, window, cx);
        } else if any.is::<SelectAndFindNext>() {
            self.select_and_find_next(&SelectAndFindNext, window, cx);
        } else if any.is::<SelectAndFindPrev>() {
            self.select_and_find_prev(&SelectAndFindPrev, window, cx);
        } else if any.is::<Save>() {
            self.save(&Save, window, cx);
        }
    }

    fn playback_macro(&mut self, _: &PlaybackMacro, window: &mut Window, cx: &mut Context<Self>) {
        Self::ensure_recorder(cx);
        let steps = cx.global::<MacroRecorder>().last.clone();
        self.run_macro(&steps, 1, window, cx);
    }

    fn run_saved_macro(&mut self, a: &RunSavedMacro, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.state.macros.get(a.index).cloned() else { return };
        let steps = steps_from_saved(&m, cx);
        self.run_macro(&steps, 1, window, cx);
    }

    fn save_macro(&mut self, _: &SaveMacro, window: &mut Window, cx: &mut Context<Self>) {
        Self::ensure_recorder(cx);
        if cx.global::<MacroRecorder>().last.is_empty() {
            self.notify_message("Record a macro first (Macro > Start Recording).", cx);
            return;
        }
        let view = cx.new(|cx| dialogs::InputDialog::new(self.weak_self.clone(), "Save Macro", "Name:", "", dialogs::InputPurpose::SaveMacro, window, cx));
        self.open_modal(view, window, cx);
    }

    pub(crate) fn save_current_macro_as(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        Self::ensure_recorder(cx);
        let steps: Vec<SavedStep> = cx
            .global::<MacroRecorder>()
            .last
            .iter()
            .map(|s| match s {
                MacroStep::Insert(t) => SavedStep::Insert(t.clone()),
                MacroStep::Action(a) => SavedStep::Action(a.name().to_string()),
            })
            .collect();
        self.state.macros.retain(|m| m.name != name);
        self.state.macros.push(SavedMacro { name: name.to_string(), steps });
        self.state.save();
        self.refresh_menus(window, cx);
        self.notify_message(format!("Macro \"{name}\" saved"), cx);
    }

    fn run_macro_multiple(&mut self, _: &RunMacroMultiple, window: &mut Window, cx: &mut Context<Self>) {
        let names: Vec<String> = std::iter::once("Current recorded macro".to_string()).chain(self.state.macros.iter().map(|m| m.name.clone())).collect();
        let view = cx.new(|cx| dialogs::RunMacroDialog::new(self.weak_self.clone(), names, window, cx));
        self.open_modal(view, window, cx);
    }

    pub(crate) fn run_macro_choice(&mut self, choice: usize, times: usize, window: &mut Window, cx: &mut Context<Self>) {
        Self::ensure_recorder(cx);
        let steps = if choice == 0 {
            cx.global::<MacroRecorder>().last.clone()
        } else {
            match self.state.macros.get(choice - 1).cloned() {
                Some(m) => steps_from_saved(&m, cx),
                None => return,
            }
        };
        self.run_macro(&steps, times, window, cx);
    }

    fn trim_trailing_and_save(&mut self, _: &TrimTrailingAndSave, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            e.update(cx, |e, cx| {
                let len = e.buf(cx).len_chars();
                let sel = e.selections.clone();
                e.set_selections(vec![Selection::caret(0)], cx);
                e.trim_trailing_space(window, cx);
                let len2 = e.buf(cx).len_chars();
                let sel: Vec<Selection> = sel.into_iter().map(|s| Selection::new(s.anchor.min(len2), s.head.min(len2))).collect();
                let _ = len;
                e.set_selections(sel, cx);
            });
            self.save(&Save, window, cx);
        }
    }

    // ------------------------------------------------------------------ run

    pub(crate) fn expand_run_variables(&self, cmd: &str, cx: &gpui::App) -> String {
        let mut out = cmd.to_string();
        if let Some(e) = self.active_editor() {
            let ed = e.read(cx);
            let b = ed.buf(cx);
            let path = b.file.clone().unwrap_or_default();
            let full = path.to_string_lossy().to_string();
            let dir = path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| b.display_name());
            let stem = path.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let ext = path.extension().map(|n| format!(".{}", n.to_string_lossy())).unwrap_or_default();
            let caret = ed.caret();
            let line = b.char_to_line(caret);
            let col = caret - b.line_to_char(line);
            let word = editor_word(&ed, cx);
            let sel = ed.selected_text(cx);
            for (k, v) in [
                ("$(FULL_CURRENT_PATH)", full),
                ("$(CURRENT_DIRECTORY)", dir),
                ("$(FILE_NAME)", name),
                ("$(NAME_PART)", stem),
                ("$(EXT_PART)", ext),
                ("$(CURRENT_WORD)", if sel.is_empty() { word } else { sel }),
                ("$(CURRENT_LINE)", (line + 1).to_string()),
                ("$(CURRENT_COLUMN)", (col + 1).to_string()),
                ("$(NPP_DIRECTORY)", std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_string_lossy().to_string())).unwrap_or_default()),
            ] {
                out = out.replace(k, &v);
            }
        }
        out
    }

    fn run_command(&mut self, _: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        let history = self.state.run_history.clone();
        let view = cx.new(|cx| dialogs::RunDialog::new(self.weak_self.clone(), history, window, cx));
        self.open_modal(view, window, cx);
    }

    pub(crate) fn execute_run(&mut self, cmd: &str, cx: &mut Context<Self>) {
        crate::session::AppState::push_history(&mut self.state.run_history, cmd);
        self.state.save();
        let expanded = self.expand_run_variables(cmd, cx);
        let dir = self.active_editor().and_then(|e| e.read(cx).buf(cx).file.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf())));
        let mut c = std::process::Command::new("/bin/sh");
        c.arg("-c").arg(&expanded);
        if let Some(d) = dir {
            c.current_dir(d);
        }
        match c.spawn() {
            Ok(_) => self.notify_message(format!("Running: {expanded}"), cx),
            Err(e) => self.notify_message(format!("Run failed: {e}"), cx),
        }
    }

    fn launch_in(&mut self, app: &str, cx: &mut Context<Self>) {
        let Some(p) = self.active_editor().and_then(|e| e.read(cx).buf(cx).file.clone()) else {
            self.notify_message("Save the document first.", cx);
            return;
        };
        if std::process::Command::new("open").arg("-a").arg(app).arg(&p).spawn().is_err() {
            self.notify_message(format!("Could not launch {app}"), cx);
        }
    }
    fn launch_in_safari(&mut self, _: &LaunchInSafari, _: &mut Window, cx: &mut Context<Self>) { self.launch_in("Safari", cx) }
    fn launch_in_chrome(&mut self, _: &LaunchInChrome, _: &mut Window, cx: &mut Context<Self>) { self.launch_in("Google Chrome", cx) }
    fn launch_in_firefox(&mut self, _: &LaunchInFirefox, _: &mut Window, cx: &mut Context<Self>) { self.launch_in("Firefox", cx) }

    fn view_in_browser(&mut self, _: &ViewInBrowser, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.active_editor().and_then(|e| e.read(cx).buf(cx).file.clone()) {
            cx.open_url(&format!("file://{}", p.to_string_lossy()));
        }
    }

    fn search_on_internet(&mut self, _: &SearchOnInternet, _: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let ed = e.read(cx);
        let mut q = ed.selected_text(cx);
        if q.is_empty() {
            q = editor_word(&ed, cx);
        }
        if q.is_empty() {
            return;
        }
        let tpl = cx.global::<Settings>().search_engine.clone();
        let url = tpl.replace("$(CURRENT_WORD)", &crate::tools::url_encode(&q));
        cx.open_url(&url);
    }

    fn open_selection_as_file(&mut self, _: &OpenSelectionAsFile, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        let text = e.read(cx).selected_text(cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        let mut p = PathBuf::from(&text);
        if p.is_relative() {
            if let Some(dir) = e.read(cx).buf(cx).file.as_ref().and_then(|f| f.parent()) {
                p = dir.join(p);
            }
        }
        if p.exists() {
            self.open_paths(vec![p], window, cx);
        } else {
            self.notify_message(format!("File not found: {}", p.display()), cx);
        }
    }

    // ------------------------------------------------------------------ edit helpers

    fn column_editor(&mut self, _: &ColumnEditor, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| dialogs::ColumnEditorDialog::new(self.weak_self.clone(), window, cx));
        self.open_modal(view, window, cx);
    }

    pub(crate) fn insert_column(&mut self, texts: ColumnInsert, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        e.update(cx, |e, cx| {
            let b = e.buf(cx);
            // Target lines: rectangular selection lines, or from the caret line to the end.
            let (first_line, col, last_line) = if e.selections.len() > 1 {
                let l0 = b.char_to_line(e.selections[0].start());
                let l1 = b.char_to_line(e.selections.last().unwrap().start());
                let col = e.selections[0].start() - b.line_to_char(l0);
                (l0, col, l1)
            } else {
                let c = e.caret();
                let l = b.char_to_line(c);
                (l, c - b.line_to_char(l), b.len_lines().saturating_sub(1))
            };
            let mut edits = Vec::new();
            for (k, line) in (first_line..=last_line).enumerate() {
                let text = match &texts {
                    ColumnInsert::Text(t) => t.clone(),
                    ColumnInsert::Numbers(f) => f(k),
                };
                let start = b.line_to_char(line);
                let len = b.line_len(line);
                if col <= len {
                    edits.push((start + col..start + col, text));
                } else {
                    edits.push((start + len..start + len, format!("{}{}", " ".repeat(col - len), text)));
                }
            }
            e.apply_edits_mapping(edits, cx);
        });
    }

    fn select_all_occurrences(&mut self, _: &SelectAllOccurrences, _: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.active_editor() else { return };
        e.update(cx, |e, cx| {
            let b = e.buf(cx);
            let p = e.primary_selection();
            let r = if p.is_empty() {
                match editor::word_at_pub(b, p.head) {
                    Some(r) => r,
                    None => return,
                }
            } else {
                p.range()
            };
            let needle = b.slice_string(r.clone());
            let text = e.buffer.update(cx, |b, _| b.text_arc());
            let b = e.buf(cx);
            let mut sels = Vec::new();
            let mut from = 0;
            while let Some(i) = text[from..].find(&needle) {
                let s = from + i;
                let cs = b.byte_to_char(s);
                let ce = b.byte_to_char(s + needle.len());
                sels.push(Selection::new(cs, ce));
                from = s + needle.len().max(1);
                if sels.len() > 50_000 {
                    break;
                }
            }
            if !sels.is_empty() {
                e.set_selections_with_primary(sels, Some(r.end), cx);
            }
        });
    }

    // ------------------------------------------------------------------ compare (Plugins > Compare)

    fn compare_with_other_view(&mut self, _: &CompareWithOtherView, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(a), Some(b)) = (self.panes[0].active_item().cloned(), self.panes[1].active_item().cloned()) else {
            // Put the previous tab into the other view to compare.
            let p = &self.panes[self.active_pane];
            if p.items.len() >= 2 {
                let prev = if p.active > 0 { p.active - 1 } else { 1 };
                let ap = self.active_pane;
                let other = 1 - ap;
                let end = self.panes[other].items.len();
                self.move_tab(ap, prev, other, end, window, cx);
                self.compare_with_other_view(&CompareWithOtherView, window, cx);
            } else {
                self.notify_message("Open two documents to compare.", cx);
            }
            return;
        };
        self.clear_compare(&ClearCompare, window, cx);
        let la: Vec<String> = { let bb = a.read(cx).buf(cx); (0..bb.len_lines()).map(|l| bb.line_text(l)).collect() };
        let lb: Vec<String> = { let bb = b.read(cx).buf(cx); (0..bb.len_lines()).map(|l| bb.line_text(l)).collect() };
        let ops = crate::tools::diff_lines(&la, &lb);
        let (mut del, mut ins) = (Vec::new(), Vec::new());
        for op in ops {
            match op {
                crate::tools::DiffOp::Delete(i) => del.push(i),
                crate::tools::DiffOp::Insert(j) => ins.push(j),
                crate::tools::DiffOp::Equal(..) => {}
            }
        }
        let n = del.len() + ins.len();
        let mark_lines = |ed: &gpui::Entity<Editor>, lines: Vec<usize>, style: u8, cx: &mut Context<Self>| {
            ed.update(cx, |e, cx| {
                e.buffer.update(cx, |b, cx| {
                    for l in lines {
                        let s = b.line_to_char(l);
                        let end = b.line_end(l).max(s + 1).min(b.len_chars().max(s));
                        b.marks.push(Mark { range: s..end.max(s), style });
                        if end == s {
                            b.marks.push(Mark { range: s..s, style });
                        }
                    }
                    b.marks.retain(|m| !m.range.is_empty() || m.style >= 6);
                    cx.notify();
                });
                cx.notify();
            });
        };
        mark_lines(&a, del, 7, cx);
        mark_lines(&b, ins, 6, cx);
        self.compare_active = true;
        self.sync_vertical = true;
        self.notify_message(if n == 0 { "Files match.".to_string() } else { format!("{n} differing lines (red: only in left, green: only in right)") }, cx);
        cx.notify();
    }

    fn clear_compare(&mut self, _: &ClearCompare, _: &mut Window, cx: &mut Context<Self>) {
        for e in self.all_editors() {
            e.update(cx, |e, cx| {
                e.buffer.update(cx, |b, cx| {
                    b.marks.retain(|m| m.style < 6);
                    cx.notify();
                })
            });
        }
        self.compare_active = false;
        cx.notify();
    }

    // ------------------------------------------------------------------ window / help

    fn windows_list(&mut self, _: &WindowsList, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.new(|cx| dialogs::WindowsDialog::new(self.weak_self.clone(), window, cx));
        self.open_modal(view, window, cx);
    }

    fn about(&mut self, _: &About, window: &mut Window, cx: &mut Context<Self>) {
        let text = format!(
            "{} {}\n\nA fast, native text and source-code editor for macOS.\nWritten in Rust and drawn on the GPU with GPUI.\n\nSyntax highlighting: syntect + two-face ({} languages)\nRegular expressions: regex / fancy-regex\nText storage: ropey\n\nSee Help > Third-Party Licenses for open-source acknowledgements.",
            crate::APP_NAME,
            env!("CARGO_PKG_VERSION"),
            crate::syntax::language_names().len()
        );
        let view = cx.new(|cx| InfoDialog::new(&format!("About {}", crate::APP_NAME), &text, false, window, cx));
        self.open_modal(view, window, cx);
    }

    fn show_third_party_licenses(&mut self, _: &ShowThirdPartyLicenses, window: &mut Window, cx: &mut Context<Self>) {
        const TITLE: &str = "Third-Party Licenses";
        const TEXT: &str = include_str!("../../THIRD_PARTY_LICENSES.md");
        for (pi, pane) in self.panes.iter().enumerate() {
            if let Some(i) = pane.items.iter().position(|e| {
                let b = e.read(cx).buf(cx);
                b.file.is_none() && b.untitled_name == TITLE
            }) {
                self.activate(pi, i, window, cx);
                return;
            }
        }
        let id = self.alloc_buffer_id();
        let buffer = cx.new(|_| {
            let mut b = crate::buffer::Buffer::new(id, TITLE.into(), EolMode::Lf, TextEncoding::Utf8);
            b.set_text_initial(TEXT);
            b.set_syntax("Markdown".into());
            b.user_read_only = true;
            b
        });
        let pane = self.active_pane;
        self.add_buffer(buffer, pane, true, window, cx);
    }

    fn open_homepage(&mut self, _: &OpenHomepage, _: &mut Window, cx: &mut Context<Self>) {
        cx.open_url(crate::HOMEPAGE);
    }

    fn debug_info(&mut self, _: &DebugInfo, window: &mut Window, cx: &mut Context<Self>) {
        let s = format!(
            "{} {}\nOS: macOS {}\nArch: {}\nConfig: {}\nOpen documents: {}\nTheme: {}\nFont: {} {}pt",
            crate::APP_NAME,
            env!("CARGO_PKG_VERSION"),
            std::process::Command::new("sw_vers").arg("-productVersion").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default(),
            std::env::consts::ARCH,
            settings::config_dir().display(),
            self.unique_buffers(cx).len(),
            cx.global::<Settings>().theme,
            cx.global::<Settings>().font_family,
            cx.global::<Settings>().effective_font_size(),
        );
        let view = cx.new(|cx| InfoDialog::new("Debug Info", &s, true, window, cx));
        self.open_modal(view, window, cx);
    }

    // ------------------------------------------------------------------ context menus

    pub fn show_editor_context_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let entries = vec![
            MenuEntry::action("Cut", editor::Cut),
            MenuEntry::action("Copy", editor::Copy),
            MenuEntry::action("Paste", editor::Paste),
            MenuEntry::action("Delete", editor::Delete),
            MenuEntry::action("Select All", editor::SelectAll),
            MenuEntry::separator(),
            MenuEntry::action("Set/Extend Selection Anchor", editor::ToggleSelectionAnchor),
            MenuEntry::action("Set/Extend Column Selection Anchor", editor::ToggleColumnSelectionAnchor),
            MenuEntry::separator(),
            MenuEntry::action("Highlight All Occurrences: Color 1", editor::HighlightAll1),
            MenuEntry::action("Highlight All Occurrences: Color 2", editor::HighlightAll2),
            MenuEntry::action("Highlight All Occurrences: Color 3", editor::HighlightAll3),
            MenuEntry::action("Highlight All Occurrences: Color 4", editor::HighlightAll4),
            MenuEntry::action("Highlight All Occurrences: Color 5", editor::HighlightAll5),
            MenuEntry::action("Clear All Highlights", editor::ClearAllHighlights),
            MenuEntry::separator(),
            MenuEntry::action("UPPERCASE", editor::UpperCase),
            MenuEntry::action("lowercase", editor::LowerCase),
            MenuEntry::separator(),
            MenuEntry::action("Open File", OpenSelectionAsFile),
            MenuEntry::action("Search on Internet", SearchOnInternet),
            MenuEntry::separator(),
            MenuEntry::action("Toggle Line Comment", editor::ToggleLineComment),
            MenuEntry::action("Block Comment", editor::BlockComment),
            MenuEntry::action("Block Uncomment", editor::BlockUncomment),
            MenuEntry::separator(),
            MenuEntry::action("Hide Lines", editor::HideLines),
            MenuEntry::action("Toggle Bookmark", editor::ToggleBookmark),
        ];
        self.show_context_menu(pos, entries, cx);
    }

    pub fn show_tab_context_menu(&mut self, pos: Point<Pixels>, cx: &mut Context<Self>) {
        let (ro, has_file, pinned) = self
            .active_editor()
            .map(|e| {
                let b = e.read(cx).buf(cx);
                (b.user_read_only, b.file.is_some(), self.pinned.contains(&e.entity_id()))
            })
            .unwrap_or((false, false, false));
        let entries = vec![
            MenuEntry::action("Close", CloseTab),
            MenuEntry::action("Close Others", CloseAllButActive),
            MenuEntry::action("Close All to the Left", CloseAllToLeft),
            MenuEntry::action("Close All to the Right", CloseAllToRight),
            MenuEntry::action("Close All Unchanged", CloseAllUnchanged),
            MenuEntry::separator(),
            MenuEntry::callback(if pinned { "Unpin Tab" } else { "Pin Tab" }, |this, _, cx| {
                if let Some(e) = this.active_editor() {
                    let id = e.entity_id();
                    if !this.pinned.remove(&id) {
                        this.pinned.insert(id);
                    }
                    cx.notify();
                }
            }),
            MenuEntry::separator(),
            MenuEntry::action("Save", Save),
            MenuEntry::action("Save As…", SaveAs),
            MenuEntry::action("Rename…", RenameFile),
            MenuEntry::action("Move to Trash", DeleteFromDisk).enabled(has_file),
            MenuEntry::action("Reload", ReloadFromDisk).enabled(has_file),
            MenuEntry::action("Print…", Print),
            MenuEntry::separator(),
            MenuEntry::action("Show in Finder", OpenContainingFolder).enabled(has_file),
            MenuEntry::action("Open in Terminal", OpenTerminalHere),
            MenuEntry::separator(),
            MenuEntry::action("Read-Only", ToggleReadOnly).checked(ro),
            MenuEntry::separator(),
            MenuEntry::action("Copy File Path", CopyFullPath),
            MenuEntry::action("Copy File Name", CopyFileName),
            MenuEntry::action("Copy Folder Path", CopyDirPath),
            MenuEntry::separator(),
            MenuEntry::action("Move to Other View", MoveToOtherView),
            MenuEntry::action("Clone to Other View", CloneToOtherView),
        ];
        self.show_context_menu(pos, entries, cx);
    }
}

pub enum ColumnInsert {
    Text(String),
    Numbers(Box<dyn Fn(usize) -> String>),
}

fn editor_word(e: &Editor, cx: &gpui::App) -> String {
    let b = e.buf(cx);
    editor::word_at_pub(b, e.caret()).map(|r| b.slice_string(r)).unwrap_or_default()
}

fn steps_from_saved(m: &SavedMacro, cx: &gpui::App) -> Vec<MacroStep> {
    m.steps
        .iter()
        .filter_map(|s| match s {
            SavedStep::Insert(t) => Some(MacroStep::Insert(t.clone())),
            SavedStep::Action(name) => cx.build_action(name, None).ok().map(MacroStep::Action),
        })
        .collect()
}

