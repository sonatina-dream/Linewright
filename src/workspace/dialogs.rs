//! Modal dialogs.

use std::path::PathBuf;

use gpui::{
    div, prelude::*, px, AnyElement, App, ClipboardItem, Context, DismissEvent, Entity, EventEmitter,
    FocusHandle, Focusable, FontWeight, SharedString, Subscription, WeakEntity, Window,
};

use super::commands::ColumnInsert;
use super::Workspace;
use crate::editor::{Editor, EditorEvent};
use crate::settings::{self, Settings};
use crate::theme::AppTheme;
use crate::tools::HashKind;
use crate::ui;

macro_rules! dialog_boilerplate {
    ($t:ty) => {
        impl Focusable for $t {
            fn focus_handle(&self, _: &App) -> FocusHandle {
                self.focus_handle.clone()
            }
        }
        impl EventEmitter<DismissEvent> for $t {}
    };
}

fn dismiss_on_cancel<T: EventEmitter<DismissEvent>>(input: &Entity<Editor>, window: &mut Window, cx: &mut Context<T>) -> Subscription {
    cx.subscribe_in(input, window, |_, _, ev: &EditorEvent, _, cx| {
        if matches!(ev, EditorEvent::Cancel) {
            cx.emit(DismissEvent);
        }
    })
}

fn button_row() -> gpui::Div {
    div().flex().justify_end().gap(px(8.0)).px(px(14.0)).py(px(10.0))
}

fn body() -> gpui::Div {
    div().flex().flex_col().gap(px(10.0)).px(px(14.0)).pt(px(12.0))
}

fn stepper(
    id: &str,
    value: i64,
    min: i64,
    max: i64,
    theme: &AppTheme,
    on_change: impl Fn(i64, &mut App) + 'static,
) -> impl IntoElement {
    let on_change = std::rc::Rc::new(on_change);
    let dec = on_change.clone();
    let inc = on_change.clone();
    div()
        .flex()
        .items_center()
        .gap(px(4.0))
        .child(ui::button(SharedString::from(format!("{id}-dec")), "−", theme).min_w(px(24.0)).px(px(4.0)).on_click(move |_, _, cx| dec((value - 1).max(min), cx)))
        .child(div().min_w(px(36.0)).flex().justify_center().text_size(px(12.5)).child(value.to_string()))
        .child(ui::button(SharedString::from(format!("{id}-inc")), "+", theme).min_w(px(24.0)).px(px(4.0)).on_click(move |_, _, cx| inc((value + 1).min(max), cx)))
}

fn setting_row(label: &str, control: impl IntoElement, theme: &AppTheme) -> impl IntoElement {
    div().flex().items_center().justify_between().gap(px(12.0)).child(ui::label(label.to_string(), theme)).child(control)
}

// ====================================================================== Info

pub struct InfoDialog {
    title: SharedString,
    text_view: Entity<Editor>,
    copy: bool,
    focus_handle: FocusHandle,
    _sub: Subscription,
}
dialog_boilerplate!(InfoDialog);

impl InfoDialog {
    pub fn new(title: &str, text: &str, copy: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let text_view = cx.new(|cx| {
            let mut e = Editor::multi_line(window, cx);
            e.set_text(text, window, cx);
            e.buffer.update(cx, |b, _| {
                b.clear_history();
                b.user_read_only = false;
            });
            e.set_selections(vec![crate::buffer::Selection::caret(0)], cx);
            e
        });
        let sub = dismiss_on_cancel(&text_view, window, cx);
        InfoDialog { title: title.to_string().into(), text_view, copy, focus_handle: cx.focus_handle(), _sub: sub }
    }
}

impl Render for InfoDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let lines = self.text_view.read(cx).buf(cx).len_lines().clamp(3, 18) as f32;
        ui::dialog_frame(self.title.clone(), 560.0, &theme)
            .track_focus(&self.focus_handle)
            .child(body().child(ui::text_area(&self.text_view, lines * 19.0 + 12.0, &theme, cx)))
            .child(
                button_row()
                    .when(self.copy, |d| {
                        d.child(ui::button("info-copy", "Copy to Clipboard", &theme).on_click(cx.listener(|this, _, _, cx| {
                            let t = this.text_view.read(cx).text(cx);
                            cx.write_to_clipboard(ClipboardItem::new_string(t));
                        })))
                    })
                    .child(ui::primary_button("info-ok", "OK", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
            )
    }
}

// ====================================================================== Input

#[derive(Clone)]
pub enum InputPurpose {
    RenameUntitled,
    SaveMacro,
    NewFileIn(PathBuf),
    NewFolderIn(PathBuf),
    RenamePath(PathBuf),
}

pub struct InputDialog {
    workspace: WeakEntity<Workspace>,
    title: SharedString,
    label: SharedString,
    input: Entity<Editor>,
    purpose: InputPurpose,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}
dialog_boilerplate!(InputDialog);

impl InputDialog {
    pub fn new(workspace: WeakEntity<Workspace>, title: &str, label: &str, initial: &str, purpose: InputPurpose, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            let mut e = Editor::single_line("", window, cx);
            e.set_text(initial, window, cx);
            e
        });
        let s1 = dismiss_on_cancel(&input, window, cx);
        let s2 = cx.subscribe_in(&input, window, |this, _, ev: &EditorEvent, window, cx| {
            if matches!(ev, EditorEvent::Confirm { .. }) {
                this.confirm(window, cx);
            }
        });
        window.focus(&input.focus_handle(cx));
        InputDialog { workspace, title: title.to_string().into(), label: label.to_string().into(), input, purpose, focus_handle: cx.focus_handle(), _subs: vec![s1, s2] }
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).text(cx).trim().to_string();
        if value.is_empty() {
            return;
        }
        let purpose = self.purpose.clone();
        let Some(ws) = self.workspace.upgrade() else { return };
        cx.emit(DismissEvent);
        ws.update(cx, |ws, cx| match purpose {
            InputPurpose::RenameUntitled => {
                if let Some(e) = ws.active_editor() {
                    e.update(cx, |e, cx| e.buffer.update(cx, |b, cx| {
                        b.untitled_name = value.clone();
                        cx.notify();
                    }));
                }
                cx.notify();
            }
            InputPurpose::SaveMacro => ws.save_current_macro_as(&value, window, cx),
            InputPurpose::NewFileIn(dir) => {
                let p = dir.join(&value);
                match std::fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                    Ok(_) => {
                        ws.folder.update(cx, |f, cx| f.refresh(cx));
                        ws.open_paths(vec![p], window, cx);
                    }
                    Err(e) => ws.notify_message(format!("Cannot create file: {e}"), cx),
                }
            }
            InputPurpose::NewFolderIn(dir) => match std::fs::create_dir(dir.join(&value)) {
                Ok(()) => ws.folder.update(cx, |f, cx| f.refresh(cx)),
                Err(e) => ws.notify_message(format!("Cannot create folder: {e}"), cx),
            },
            InputPurpose::RenamePath(path) => {
                let new_path = path.with_file_name(&value);
                match std::fs::rename(&path, &new_path) {
                    Ok(()) => {
                        for e in ws.all_editors() {
                            let matches = e.read(cx).buf(cx).file.as_deref() == Some(path.as_path());
                            if matches {
                                let np = new_path.clone();
                                e.update(cx, |e, cx| e.buffer.update(cx, |b, cx| {
                                    b.file = Some(np);
                                    cx.notify();
                                }));
                            }
                        }
                        ws.folder.update(cx, |f, cx| f.refresh(cx));
                        cx.notify();
                    }
                    Err(e) => ws.notify_message(format!("Rename failed: {e}"), cx),
                }
            }
        });
    }
}

impl Render for InputDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        ui::dialog_frame(self.title.clone(), 420.0, &theme)
            .track_focus(&self.focus_handle)
            .child(body().child(ui::label(self.label.clone(), &theme)).child(ui::input(&self.input, &theme, cx)))
            .child(
                button_row()
                    .child(ui::button("in-cancel", "Cancel", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))))
                    .child(ui::primary_button("in-ok", "OK", &theme).on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx)))),
            )
    }
}

// ====================================================================== Go To

pub struct GoToDialog {
    workspace: WeakEntity<Workspace>,
    editor: Entity<Editor>,
    input: Entity<Editor>,
    by_offset: bool,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}
dialog_boilerplate!(GoToDialog);

impl GoToDialog {
    pub fn new(workspace: WeakEntity<Workspace>, editor: Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| Editor::single_line("", window, cx));
        let s1 = dismiss_on_cancel(&input, window, cx);
        let s2 = cx.subscribe_in(&input, window, |this, _, ev: &EditorEvent, window, cx| {
            if matches!(ev, EditorEvent::Confirm { .. }) {
                this.go(window, cx);
            }
        });
        window.focus(&input.focus_handle(cx));
        GoToDialog { workspace, editor, input, by_offset: false, focus_handle: cx.focus_handle(), _subs: vec![s1, s2] }
    }

    fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).text(cx);
        let Ok(n) = text.trim().parse::<usize>() else { return };
        let by_offset = self.by_offset;
        self.editor.update(cx, |e, cx| {
            if by_offset {
                let len = e.buf(cx).len_chars();
                let p = n.min(len);
                e.select_range(p..p, true, cx);
            } else {
                e.go_to_line(n.saturating_sub(1), cx);
            }
        });
        let _ = &self.workspace;
        window.focus(&self.editor.focus_handle(cx));
        cx.emit(DismissEvent);
    }
}

impl Render for GoToDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let (here, max) = {
            let e = self.editor.read(cx);
            let b = e.buf(cx);
            if self.by_offset {
                (e.caret(), b.len_chars())
            } else {
                (b.char_to_line(e.caret()) + 1, b.len_lines())
            }
        };
        ui::dialog_frame("Go To…", 360.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                body()
                    .child(
                        div()
                            .flex()
                            .gap(px(16.0))
                            .child(ui::radio("goto-line", "Line", !self.by_offset, &theme).on_click(cx.listener(|this, _, _, cx| {
                                this.by_offset = false;
                                cx.notify();
                            })))
                            .child(ui::radio("goto-off", "Offset", self.by_offset, &theme).on_click(cx.listener(|this, _, _, cx| {
                                this.by_offset = true;
                                cx.notify();
                            }))),
                    )
                    .child(setting_row("Current:", ui::label(here.to_string(), &theme), &theme))
                    .child(div().flex().items_center().gap(px(8.0)).child(ui::label("Go to:", &theme)).child(ui::input(&self.input, &theme, cx).flex_1()))
                    .child(setting_row("Maximum:", ui::label(max.to_string(), &theme), &theme)),
            )
            .child(
                button_row()
                    .child(ui::button("goto-cancel", "Cancel", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))))
                    .child(ui::primary_button("goto-go", "Go", &theme).on_click(cx.listener(|this, _, window, cx| this.go(window, cx)))),
            )
    }
}

// ====================================================================== Preferences

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrefPage {
    General,
    Editing,
    Display,
    NewDocument,
    Highlighting,
    AutoCompletion,
    Backup,
    Searching,
    Misc,
}

pub struct PreferencesDialog {
    page: PrefPage,
    font_input: Entity<Editor>,
    engine_input: Entity<Editor>,
    date_input: Entity<Editor>,
    dir_input: Entity<Editor>,
    lang_input: Entity<Editor>,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}
dialog_boilerplate!(PreferencesDialog);

impl PreferencesDialog {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let s = cx.global::<Settings>().clone();
        let mk = |text: &str, window: &mut Window, cx: &mut Context<Self>| {
            let t = text.to_string();
            cx.new(|cx| {
                let mut e = Editor::single_line("", window, cx);
                e.set_text(&t, window, cx);
                e
            })
        };
        let font_input = mk(&s.font_family, window, cx);
        let engine_input = mk(&s.search_engine, window, cx);
        let date_input = mk(&s.date_time_format, window, cx);
        let dir_input = mk(&s.default_directory, window, cx);
        let lang_input = mk(&crate::syntax::display_name(&s.new_doc_language), window, cx);
        let mut subs = Vec::new();
        for (input, field) in [(&font_input, 0u8), (&engine_input, 1), (&date_input, 2), (&dir_input, 3), (&lang_input, 4)] {
            subs.push(dismiss_on_cancel(input, window, cx));
            subs.push(cx.subscribe_in(input, window, move |_, input, ev: &EditorEvent, _, cx| {
                if matches!(ev, EditorEvent::Confirm { .. } | EditorEvent::Blurred) {
                    let v = input.read(cx).text(cx);
                    settings::update_settings(cx, |s| match field {
                        0 => {
                            if !v.trim().is_empty() {
                                s.font_family = v.trim().to_string()
                            }
                        }
                        1 => s.search_engine = v,
                        2 => s.date_time_format = v,
                        3 => s.default_directory = v,
                        _ => {
                            let names = crate::syntax::language_names();
                            if let Some(n) = names.iter().find(|n| n.eq_ignore_ascii_case(v.trim())) {
                                s.new_doc_language = n.clone();
                            } else {
                                s.new_doc_language = crate::syntax::PLAIN_TEXT.into();
                            }
                        }
                    });
                }
            }));
        }
        subs.push(cx.observe_global::<Settings>(|_, cx| cx.notify()));
        PreferencesDialog { page: PrefPage::General, font_input, engine_input, date_input, dir_input, lang_input, focus_handle: cx.focus_handle(), _subs: subs }
    }

    fn check(&self, id: &'static str, label: &'static str, get: fn(&Settings) -> bool, set: fn(&mut Settings, bool), theme: &AppTheme, cx: &App) -> impl IntoElement {
        let on = get(cx.global::<Settings>());
        ui::checkbox(id, label, on, theme).on_click(move |_, _, cx| settings::update_settings(cx, |s| set(s, !on)))
    }

    fn page_content(&self, theme: &AppTheme, cx: &mut Context<Self>) -> AnyElement {
        let s = cx.global::<Settings>().clone();
        let c = div().flex().flex_col().gap(px(9.0));
        match self.page {
            PrefPage::General => c
                .child(ui::section_title("INTERFACE", theme))
                .child(self.check("p-tb", "Show toolbar", |s| s.show_toolbar, |s, v| s.show_toolbar = v, theme, cx))
                .child(self.check("p-sb", "Show status bar", |s| s.show_status_bar, |s, v| s.show_status_bar = v, theme, cx))
                .child(self.check("p-tab", "Show tab bar", |s| s.show_tab_bar, |s, v| s.show_tab_bar = v, theme, cx))
                .child(self.check("p-close", "Show close button on each tab", |s| s.tab_close_button, |s, v| s.tab_close_button = v, theme, cx))
                .child(self.check("p-dbl", "Double click to close document", |s| s.double_click_closes_tab, |s, v| s.double_click_closes_tab = v, theme, cx))
                .child(setting_row("UI font size", stepper("p-uisize", s.ui_font_size as i64, 10, 20, theme, |v, cx| settings::update_settings(cx, |s| s.ui_font_size = v as f32)), theme))
                .child(ui::section_title("FILES", theme))
                .child(self.check("p-detect", "Detect file changes made by other programs", |s| s.file_change_detection, |s, v| s.file_change_detection = v, theme, cx))
                .child(self.check("p-autoreload", "Update silently (reload unmodified documents)", |s| s.auto_reload_unmodified, |s, v| s.auto_reload_unmodified = v, theme, cx))
                .child(setting_row("Max. recent files", stepper("p-recent", s.max_recent_files as i64, 0, 50, theme, |v, cx| settings::update_settings(cx, |s| s.max_recent_files = v as usize)), theme))
                .into_any_element(),
            PrefPage::Editing => c
                .child(ui::section_title("FONT", theme))
                .child(div().flex().items_center().gap(px(8.0)).child(ui::label("Font family", theme)).child(ui::input(&self.font_input, theme, cx).flex_1()))
                .child(setting_row("Font size", stepper("p-fsize", s.font_size as i64, 6, 48, theme, |v, cx| settings::update_settings(cx, |s| s.font_size = v as f32)), theme))
                .child(ui::section_title("TABS & INDENTATION", theme))
                .child(setting_row("Tab size", stepper("p-tabsize", s.tab_size as i64, 1, 16, theme, |v, cx| settings::update_settings(cx, |s| s.tab_size = v as usize)), theme))
                .child(self.check("p-spaces", "Replace tabs by spaces", |s| s.use_spaces, |s, v| s.use_spaces = v, theme, cx))
                .child(self.check("p-autoindent", "Auto-indent", |s| s.auto_indent, |s, v| s.auto_indent = v, theme, cx))
                .child(ui::section_title("CARET & SELECTION", theme))
                .child(setting_row("Caret width", stepper("p-caret", s.caret_width as i64, 1, 4, theme, |v, cx| settings::update_settings(cx, |s| s.caret_width = v as f32)), theme))
                .child(self.check("p-blink", "Blinking caret", |s| s.caret_blink, |s, v| s.caret_blink = v, theme, cx))
                .child(self.check("p-multi", "Multi-editing (⌘+click)", |s| s.multi_editing, |s, v| s.multi_editing = v, theme, cx))
                .child(self.check("p-copyline", "Copy / cut the current line when nothing is selected", |s| s.copy_line_without_selection, |s, v| s.copy_line_without_selection = v, theme, cx))
                .child(self.check("p-beyond", "Allow scrolling beyond the last line", |s| s.scroll_beyond_last_line, |s, v| s.scroll_beyond_last_line = v, theme, cx))
                .child(ui::section_title("VERTICAL EDGE", theme))
                .child(self.check("p-edge", "Show vertical edge", |s| s.show_edge, |s, v| s.show_edge = v, theme, cx))
                .child(setting_row("Edge column", stepper("p-edgecol", s.edge_column as i64, 10, 400, theme, |v, cx| settings::update_settings(cx, |s| s.edge_column = v as usize)), theme))
                .into_any_element(),
            PrefPage::Display => c
                .child(self.check("p-ln", "Display line numbers", |s| s.show_line_numbers, |s, v| s.show_line_numbers = v, theme, cx))
                .child(self.check("p-bm", "Display bookmark margin", |s| s.show_bookmark_margin, |s, v| s.show_bookmark_margin = v, theme, cx))
                .child(self.check("p-fold", "Display folding margin", |s| s.show_fold_margin, |s, v| s.show_fold_margin = v, theme, cx))
                .child(self.check("p-history", "Display change history", |s| s.show_change_history, |s, v| s.show_change_history = v, theme, cx))
                .child(self.check("p-guides", "Show indent guides", |s| s.show_indent_guides, |s, v| s.show_indent_guides = v, theme, cx))
                .child(self.check("p-ws", "Show spaces and tabs", |s| s.show_whitespace, |s, v| s.show_whitespace = v, theme, cx))
                .child(self.check("p-eol", "Show end of line", |s| s.show_eol, |s, v| s.show_eol = v, theme, cx))
                .child(self.check("p-wrap", "Word wrap", |s| s.word_wrap, |s, v| s.word_wrap = v, theme, cx))
                .child(self.check("p-wrapsym", "Show wrap symbol", |s| s.show_wrap_symbol, |s, v| s.show_wrap_symbol = v, theme, cx))
                .child(self.check("p-curline", "Highlight current line", |s| s.highlight_current_line, |s, v| s.highlight_current_line = v, theme, cx))
                .child(self.check("p-frame", "Current line: frame instead of background", |s| s.current_line_frame, |s, v| s.current_line_frame = v, theme, cx))
                .into_any_element(),
            PrefPage::NewDocument => {
                let eol = s.new_doc_eol.clone();
                let enc = s.new_doc_encoding.clone();
                let eol_radio = |id: &'static str, label: &'static str, val: &'static str| {
                    ui::radio(id, label, eol == val, theme).on_click(move |_, _, cx| settings::update_settings(cx, |s| s.new_doc_eol = val.into()))
                };
                let enc_radio = |id: &'static str, label: &'static str, val: &'static str| {
                    ui::radio(id, label, enc == val, theme).on_click(move |_, _, cx| settings::update_settings(cx, |s| s.new_doc_encoding = val.into()))
                };
                c.child(ui::section_title("FORMAT (LINE ENDING)", theme))
                    .child(div().flex().gap(px(14.0)).child(eol_radio("p-crlf", "CRLF (Windows)", "crlf")).child(eol_radio("p-lf", "LF (Unix and macOS)", "lf")).child(eol_radio("p-cr", "CR (classic Mac OS)", "cr")))
                    .child(ui::section_title("ENCODING", theme))
                    .child(enc_radio("p-ansi", "ANSI", "windows-1252"))
                    .child(enc_radio("p-utf8", "UTF-8", "utf-8"))
                    .child(enc_radio("p-utf8bom", "UTF-8 with BOM", "utf-8-bom"))
                    .child(enc_radio("p-utf16be", "UTF-16 Big Endian with BOM", "utf-16be-bom"))
                    .child(enc_radio("p-utf16le", "UTF-16 Little Endian with BOM", "utf-16le-bom"))
                    .child(ui::section_title("DEFAULT LANGUAGE", theme))
                    .child(ui::input(&self.lang_input, theme, cx))
                    .child(ui::muted("e.g. Python, Rust, Markdown — or \"Plain Text\"", theme))
                    .into_any_element()
            }
            PrefPage::Highlighting => c
                .child(ui::section_title("SMART HIGHLIGHTING", theme))
                .child(self.check("p-smart", "Enable smart highlighting", |s| s.smart_highlight, |s, v| s.smart_highlight = v, theme, cx))
                .child(self.check("p-smartcase", "Match case", |s| s.smart_highlight_match_case, |s, v| s.smart_highlight_match_case = v, theme, cx))
                .child(self.check("p-smartword", "Match whole word only", |s| s.smart_highlight_whole_word, |s, v| s.smart_highlight_whole_word = v, theme, cx))
                .child(ui::section_title("MATCHING", theme))
                .child(self.check("p-brace", "Highlight matching braces", |s| s.brace_highlight, |s, v| s.brace_highlight = v, theme, cx))
                .into_any_element(),
            PrefPage::AutoCompletion => c
                .child(self.check("p-ac", "Enable word completion on each input", |s| s.auto_completion, |s, v| s.auto_completion = v, theme, cx))
                .child(setting_row("From Nth character", stepper("p-acmin", s.auto_completion_min_chars as i64, 1, 9, theme, |v, cx| settings::update_settings(cx, |s| s.auto_completion_min_chars = v as usize)), theme))
                .child(ui::section_title("AUTO-INSERT", theme))
                .child(self.check("p-brackets", "( [ {  brackets", |s| s.auto_close_brackets, |s, v| s.auto_close_brackets = v, theme, cx))
                .child(self.check("p-quotes", "\" ' `  quotes", |s| s.auto_close_quotes, |s, v| s.auto_close_quotes = v, theme, cx))
                .child(self.check("p-tags", "HTML/XML close tag", |s| s.auto_close_html_tags, |s, v| s.auto_close_html_tags = v, theme, cx))
                .into_any_element(),
            PrefPage::Backup => c
                .child(self.check("p-session", "Remember current session for next launch", |s| s.remember_session, |s, v| s.remember_session = v, theme, cx))
                .child(self.check("p-snapshot", "Enable session snapshot and periodic backup", |s| s.session_snapshot, |s, v| s.session_snapshot = v, theme, cx))
                .child(setting_row("Backup every (seconds)", stepper("p-backup", s.backup_interval_secs as i64, 2, 300, theme, |v, cx| settings::update_settings(cx, |s| s.backup_interval_secs = v as u64)), theme))
                .child(ui::muted(format!("Backups are stored in {}", settings::backup_dir().display()), theme))
                .child(ui::section_title("ON SAVE", theme))
                .child(self.check("p-trim", "Trim trailing whitespace on save", |s| s.trim_trailing_on_save, |s, v| s.trim_trailing_on_save = v, theme, cx))
                .child(self.check("p-final", "Ensure a final newline on save", |s| s.ensure_final_newline, |s, v| s.ensure_final_newline = v, theme, cx))
                .into_any_element(),
            PrefPage::Searching => c
                .child(self.check("p-fill", "Fill Find field with selected text", |s| s.find_dialog_fill_from_selection, |s, v| s.find_dialog_fill_from_selection = v, theme, cx))
                .child(ui::section_title("SEARCH ENGINE (Edit > On Selection > Search on Internet)", theme))
                .child(ui::input(&self.engine_input, theme, cx))
                .child(ui::muted("$(CURRENT_WORD) is replaced by the selected text", theme))
                .into_any_element(),
            PrefPage::Misc => c
                .child(ui::section_title("DATE TIME (Edit > Insert > Date Time (customized))", theme))
                .child(ui::input(&self.date_input, theme, cx))
                .child(ui::muted("strftime format, e.g. %Y-%m-%d %H:%M", theme))
                .child(ui::section_title("DEFAULT DIRECTORY", theme))
                .child(ui::input(&self.dir_input, theme, cx))
                .child(ui::muted("Used by Open/Save dialogs when no document is active", theme))
                .into_any_element(),
        }
    }
}

impl Render for PreferencesDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let pages = [
            (PrefPage::General, "General"),
            (PrefPage::Editing, "Editing"),
            (PrefPage::Display, "Margins & Display"),
            (PrefPage::NewDocument, "New Document"),
            (PrefPage::Highlighting, "Highlighting"),
            (PrefPage::AutoCompletion, "Auto-Completion"),
            (PrefPage::Backup, "Backup & Session"),
            (PrefPage::Searching, "Searching"),
            (PrefPage::Misc, "Other"),
        ];
        let content = self.page_content(&theme, cx);
        let nav: Vec<_> = pages.iter().enumerate().map(|(i, (p, label))| {
            let p = *p;
            let active = self.page == p;
            div()
                .id(("pref-nav", i))
                .px(px(10.0))
                .py(px(5.0))
                .rounded(px(4.0))
                .text_size(px(12.5))
                .cursor_pointer()
                .when(active, |d| d.bg(theme.list_selected).font_weight(FontWeight::MEDIUM))
                .hover(|d| d.bg(theme.list_hover))
                .child(*label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.page = p;
                    cx.notify();
                }))
        }).collect();
        ui::dialog_frame("Preferences", 720.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                div()
                    .flex()
                    .h(px(440.0))
                    .child(div().w(px(170.0)).flex_shrink_0().p(px(8.0)).flex().flex_col().gap(px(2.0)).border_r_1().border_color(theme.chrome_border).children(nav))
                    .child(div().id("pref-content").flex_1().p(px(16.0)).overflow_y_scroll().child(content)),
            )
            .child(
                div()
                    .border_t_1()
                    .border_color(theme.chrome_border)
                    .child(button_row().child(ui::primary_button("pref-close", "Close", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))))),
            )
    }
}

// ====================================================================== Style configurator

pub struct StyleDialog {
    focus_handle: FocusHandle,
    /// (name, background, foreground, selection, current line) per theme.
    swatches: Vec<(String, gpui::Hsla, gpui::Hsla, gpui::Hsla, gpui::Hsla)>,
    _sub: Subscription,
}
dialog_boilerplate!(StyleDialog);

impl StyleDialog {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sub = cx.observe_global::<crate::theme::ThemeState>(|_, cx| cx.notify());
        let swatches = crate::theme::theme_names()
            .into_iter()
            .map(|n| {
                let t = crate::theme::AppTheme::load(&n);
                (n, t.bg, t.fg, t.selection_bg, t.current_line_bg)
            })
            .collect();
        StyleDialog { focus_handle: cx.focus_handle(), swatches, _sub: sub }
    }
}

impl Render for StyleDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let current = cx.global::<Settings>().theme.clone();
        let s = cx.global::<Settings>().clone();
        let rows = self.swatches.clone().into_iter().enumerate().map(|(i, (name, p_bg, p_fg, p_sel, p_line))| {
            let active = name == current;
            let n2 = name.clone();
            div()
                .id(("theme", i))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .cursor_pointer()
                .when(active, |d| d.bg(theme.list_selected))
                .hover(|d| d.bg(theme.list_hover))
                .child(
                    div()
                        .flex()
                        .w(px(64.0))
                        .h(px(16.0))
                        .rounded(px(3.0))
                        .border_1()
                        .border_color(theme.chrome_border)
                        .overflow_hidden()
                        .child(div().flex_1().bg(p_bg))
                        .child(div().w(px(10.0)).bg(p_fg))
                        .child(div().w(px(10.0)).bg(p_sel))
                        .child(div().w(px(10.0)).bg(p_line)),
                )
                .child(div().text_size(px(12.5)).child(name.clone()))
                .on_click(move |_, window, cx| window.dispatch_action(Box::new(crate::actions::SetTheme { name: n2.clone() }), cx))
        });
        ui::dialog_frame("Themes", 520.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                body()
                    .child(ui::section_title("SELECT THEME", &theme))
                    .child(div().id("theme-list").h(px(340.0)).overflow_y_scroll().border_1().border_color(theme.chrome_border).rounded(px(4.0)).p(px(4.0)).children(rows))
                    .child(setting_row("Font size", stepper("st-fsize", s.font_size as i64, 6, 48, &theme, |v, cx| settings::update_settings(cx, |s| s.font_size = v as f32)), &theme))
                    .child(ui::muted("Font family and more options: Settings > Preferences > Editing", &theme)),
            )
            .child(button_row().child(ui::primary_button("st-close", "Close", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))))
    }
}

// ====================================================================== Shortcuts list

pub struct ShortcutsDialog {
    filter: Entity<Editor>,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}
dialog_boilerplate!(ShortcutsDialog);

impl ShortcutsDialog {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| Editor::single_line("Filter commands or keys", window, cx));
        let s1 = dismiss_on_cancel(&filter, window, cx);
        let s2 = cx.subscribe(&filter, |_, _, ev: &EditorEvent, cx| {
            if matches!(ev, EditorEvent::Edited) {
                cx.notify();
            }
        });
        window.focus(&filter.focus_handle(cx));
        ShortcutsDialog { filter, focus_handle: cx.focus_handle(), _subs: vec![s1, s2] }
    }
}

pub fn humanize_action(name: &str) -> String {
    let short = name.rsplit("::").next().unwrap_or(name);
    let mut out = String::new();
    for (i, c) in short.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
        }
        out.push(c);
    }
    out
}

impl Render for ShortcutsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let q = self.filter.read(cx).text(cx).to_lowercase();
        let mut rows: Vec<(String, String, String)> = Vec::new();
        for b in cx.key_bindings().borrow().bindings() {
            let keys: Vec<String> = b.keystrokes().iter().map(|k| k.unparse()).collect();
            let keys = keys.join(" ");
            let name = humanize_action(b.action().name());
            let ctx = b.predicate().map(|p| p.to_string()).unwrap_or_default();
            if name.contains("Completion") {
                continue;
            }
            if q.is_empty() || name.to_lowercase().contains(&q) || keys.to_lowercase().contains(&q) {
                rows.push((name, keys, ctx));
            }
        }
        rows.sort();
        rows.dedup();
        let items = rows.into_iter().enumerate().map(|(i, (name, keys, ctx))| {
            div()
                .id(("sc", i))
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(8.0))
                .h(px(22.0))
                .text_size(px(12.5))
                .hover(|d| d.bg(theme.list_hover))
                .child(div().flex_1().child(name))
                .child(div().text_color(theme.muted_fg).text_size(px(11.0)).child(if ctx.contains("full") { "editor" } else if ctx.contains("Editor") { "text" } else { "" }))
                .child(div().w(px(170.0)).font_family("Menlo").text_size(px(12.0)).text_color(theme.accent).child(keys))
        });
        ui::dialog_frame("Keyboard Shortcuts", 620.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                body()
                    .child(ui::input(&self.filter, &theme, cx))
                    .child(div().id("sc-list").h(px(380.0)).overflow_y_scroll().border_1().border_color(theme.chrome_border).rounded(px(4.0)).children(items)),
            )
            .child(
                button_row()
                    .child(ui::button("sc-edit", "Edit keymap.json…", &theme).on_click(cx.listener(|_, _, window, cx| {
                        cx.emit(DismissEvent);
                        window.dispatch_action(Box::new(crate::actions::EditKeymapFile), cx);
                    })))
                    .child(ui::primary_button("sc-close", "Close", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
            )
    }
}

// ====================================================================== Hash generator

pub struct HashDialog {
    kind: HashKind,
    input: Entity<Editor>,
    per_line: bool,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}
dialog_boilerplate!(HashDialog);

impl HashDialog {
    pub fn new(kind: HashKind, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| Editor::multi_line(window, cx));
        let s1 = dismiss_on_cancel(&input, window, cx);
        let s2 = cx.subscribe(&input, |_, _, ev: &EditorEvent, cx| {
            if matches!(ev, EditorEvent::Edited) {
                cx.notify();
            }
        });
        window.focus(&input.focus_handle(cx));
        HashDialog { kind, input, per_line: false, focus_handle: cx.focus_handle(), _subs: vec![s1, s2] }
    }

    fn output(&self, cx: &App) -> String {
        let text = self.input.read(cx).text(cx);
        if text.is_empty() {
            return String::new();
        }
        if self.per_line {
            text.lines().map(|l| crate::tools::hash_hex(l.as_bytes(), self.kind)).collect::<Vec<_>>().join("\n")
        } else {
            crate::tools::hash_hex(text.as_bytes(), self.kind)
        }
    }
}

impl Render for HashDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let out = self.output(cx);
        let kind = self.kind;
        ui::dialog_frame(format!("Generate {}", kind.label()), 600.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                body()
                    .child(ui::label("Text:", &theme))
                    .child(ui::text_area(&self.input, 130.0, &theme, cx))
                    .child(ui::checkbox("hash-lines", "Treat each line as a separate string", self.per_line, &theme).on_click(cx.listener(|this, _, _, cx| {
                        this.per_line = !this.per_line;
                        cx.notify();
                    })))
                    .child(ui::label(format!("{}:", kind.label()), &theme))
                    .child(
                        div()
                            .id("hash-out")
                            .h(px(90.0))
                            .p(px(6.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(theme.input_border)
                            .bg(theme.input_bg)
                            .overflow_y_scroll()
                            .font_family("Menlo")
                            .text_size(px(12.0))
                            .children(out.lines().map(|l| div().child(l.to_string())).collect::<Vec<_>>()),
                    ),
            )
            .child(
                button_row()
                    .child(ui::button("hash-copy", "Copy to Clipboard", &theme).on_click(cx.listener(move |this, _, _, cx| {
                        let o = this.output(cx);
                        cx.write_to_clipboard(ClipboardItem::new_string(o));
                    })))
                    .child(ui::primary_button("hash-close", "Close", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))),
            )
    }
}

// ====================================================================== Run

pub struct RunDialog {
    workspace: WeakEntity<Workspace>,
    input: Entity<Editor>,
    history: Vec<String>,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}
dialog_boilerplate!(RunDialog);

impl RunDialog {
    pub fn new(workspace: WeakEntity<Workspace>, history: Vec<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let first = history.first().cloned().unwrap_or_default();
        let input = cx.new(|cx| {
            let mut e = Editor::single_line("open -a Safari \"$(FULL_CURRENT_PATH)\"", window, cx);
            e.set_text(&first, window, cx);
            e
        });
        let s1 = dismiss_on_cancel(&input, window, cx);
        let s2 = cx.subscribe_in(&input, window, |this, _, ev: &EditorEvent, _, cx| {
            if matches!(ev, EditorEvent::Confirm { .. }) {
                this.run(cx);
            }
        });
        window.focus(&input.focus_handle(cx));
        RunDialog { workspace, input, history, focus_handle: cx.focus_handle(), _subs: vec![s1, s2] }
    }

    fn run(&mut self, cx: &mut Context<Self>) {
        let cmd = self.input.read(cx).text(cx);
        if cmd.trim().is_empty() {
            return;
        }
        if let Some(ws) = self.workspace.upgrade() {
            ws.update(cx, |ws, cx| ws.execute_run(&cmd, cx));
        }
        cx.emit(DismissEvent);
    }
}

impl Render for RunDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let hist = self.history.iter().take(8).enumerate().map(|(i, h)| {
            let h2 = h.clone();
            div()
                .id(("run-h", i))
                .px(px(6.0))
                .py(px(2.0))
                .rounded(px(3.0))
                .font_family("Menlo")
                .text_size(px(11.5))
                .cursor_pointer()
                .hover(|d| d.bg(theme.list_hover))
                .child(h.clone())
                .on_click(cx.listener(move |this, _, window, cx| {
                    let h = h2.clone();
                    this.input.update(cx, |e, cx| e.set_text(&h, window, cx));
                }))
        });
        ui::dialog_frame("Run…", 560.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                body()
                    .child(ui::label("The program to run (shell command):", &theme))
                    .child(ui::input(&self.input, &theme, cx))
                    .child(ui::muted("Variables: $(FULL_CURRENT_PATH) $(CURRENT_DIRECTORY) $(FILE_NAME) $(NAME_PART) $(EXT_PART) $(CURRENT_WORD) $(CURRENT_LINE) $(CURRENT_COLUMN)", &theme))
                    .when(!self.history.is_empty(), |d| d.child(ui::section_title("HISTORY", &theme)).child(div().flex().flex_col().children(hist))),
            )
            .child(
                button_row()
                    .child(ui::button("run-cancel", "Cancel", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))))
                    .child(ui::primary_button("run-run", "Run", &theme).on_click(cx.listener(|this, _, _, cx| this.run(cx)))),
            )
    }
}

// ====================================================================== Run macro multiple times

pub struct RunMacroDialog {
    workspace: WeakEntity<Workspace>,
    names: Vec<String>,
    choice: usize,
    times: i64,
    until_eof: bool,
    focus_handle: FocusHandle,
}
dialog_boilerplate!(RunMacroDialog);

impl RunMacroDialog {
    pub fn new(workspace: WeakEntity<Workspace>, names: Vec<String>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        RunMacroDialog { workspace, names, choice: 0, times: 1, until_eof: false, focus_handle: cx.focus_handle() }
    }
}

impl Render for RunMacroDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let choices = self.names.iter().enumerate().map(|(i, n)| {
            ui::radio(("rm-c", i), n.clone(), self.choice == i, &theme).on_click(cx.listener(move |this, _, _, cx| {
                this.choice = i;
                cx.notify();
            }))
        });
        let entity = cx.entity();
        ui::dialog_frame("Run Macro Repeatedly", 420.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                body()
                    .child(ui::section_title("MACRO TO RUN", &theme))
                    .child(div().flex().flex_col().gap(px(6.0)).children(choices))
                    .child(ui::section_title("REPEAT", &theme))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(ui::radio("rm-n", "Run", !self.until_eof, &theme).on_click(cx.listener(|this, _, _, cx| {
                                this.until_eof = false;
                                cx.notify();
                            })))
                            .child(stepper("rm-times", self.times, 1, 100_000, &theme, move |v, cx| entity.update(cx, |this, cx| {
                                this.times = v;
                                cx.notify();
                            })))
                            .child(ui::label("times", &theme)),
                    )
                    .child(ui::radio("rm-eof", "Run until the end of file", self.until_eof, &theme).on_click(cx.listener(|this, _, _, cx| {
                        this.until_eof = true;
                        cx.notify();
                    }))),
            )
            .child(
                button_row()
                    .child(ui::button("rm-cancel", "Cancel", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))))
                    .child(ui::primary_button("rm-run", "Run", &theme).on_click(cx.listener(|this, _, window, cx| {
                        let (choice, times) = (this.choice, if this.until_eof { 0 } else { this.times as usize });
                        cx.emit(DismissEvent);
                        if let Some(ws) = this.workspace.upgrade() {
                            ws.update(cx, |ws, cx| ws.run_macro_choice(choice, times, window, cx));
                        }
                    }))),
            )
    }
}

// ====================================================================== Column editor

pub struct ColumnEditorDialog {
    workspace: WeakEntity<Workspace>,
    text_mode: bool,
    text: Entity<Editor>,
    initial: Entity<Editor>,
    step: Entity<Editor>,
    repeat: Entity<Editor>,
    leading: u8,
    format: u8,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}
dialog_boilerplate!(ColumnEditorDialog);

impl ColumnEditorDialog {
    pub fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mk = |v: &str, window: &mut Window, cx: &mut Context<Self>| {
            let v = v.to_string();
            cx.new(|cx| {
                let mut e = Editor::single_line("", window, cx);
                e.set_text(&v, window, cx);
                e
            })
        };
        let text = mk("", window, cx);
        let initial = mk("1", window, cx);
        let step = mk("1", window, cx);
        let repeat = mk("1", window, cx);
        let subs = vec![
            dismiss_on_cancel(&text, window, cx),
            dismiss_on_cancel(&initial, window, cx),
            dismiss_on_cancel(&step, window, cx),
            dismiss_on_cancel(&repeat, window, cx),
        ];
        window.focus(&text.focus_handle(cx));
        ColumnEditorDialog { workspace, text_mode: true, text, initial, step, repeat, leading: 0, format: 0, focus_handle: cx.focus_handle(), _subs: subs }
    }

    fn apply(&mut self, cx: &mut Context<Self>) {
        let insert = if self.text_mode {
            ColumnInsert::Text(self.text.read(cx).text(cx))
        } else {
            let p = |e: &Entity<Editor>, d: i64| e.read(cx).text(cx).trim().parse::<i64>().unwrap_or(d);
            let initial = p(&self.initial, 1);
            let step = p(&self.step, 1);
            let repeat = p(&self.repeat, 1).max(1) as usize;
            let leading = self.leading;
            let format = self.format;
            // Width for padding: compute lazily from a generous estimate.
            ColumnInsert::Numbers(Box::new(move |k| {
                let v = initial + step * (k / repeat) as i64;
                let s = match format {
                    1 => format!("{:X}", v),
                    2 => format!("{:o}", v),
                    3 => format!("{:b}", v),
                    _ => v.to_string(),
                };
                let width = 0usize.max(match format {
                    1 => format!("{:X}", initial + step * 1000).len(),
                    2 => format!("{:o}", initial + step * 1000).len(),
                    3 => format!("{:b}", initial + step * 1000).len(),
                    _ => (initial + step * 1000).to_string().len(),
                });
                match leading {
                    1 => format!("{:0>width$}", s),
                    2 => format!("{:>width$}", s),
                    _ => s,
                }
            }))
        };
        if let Some(ws) = self.workspace.upgrade() {
            ws.update(cx, |ws, cx| ws.insert_column(insert, cx));
        }
        cx.emit(DismissEvent);
    }
}

impl Render for ColumnEditorDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let row = |label: &'static str, e: &Entity<Editor>, cx: &App| {
            div().flex().items_center().gap(px(8.0)).child(div().w(px(110.0)).child(ui::label(label, &theme))).child(ui::input(e, &theme, cx).w(px(120.0)))
        };
        let leading = self.leading;
        let format = self.format;
        ui::dialog_frame("Insert in Column", 440.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                body()
                    .child(ui::radio("ce-text", "Text to Insert", self.text_mode, &theme).on_click(cx.listener(|this, _, _, cx| {
                        this.text_mode = true;
                        cx.notify();
                    })))
                    .child(div().pl(px(20.0)).child(ui::input(&self.text, &theme, cx)))
                    .child(ui::radio("ce-num", "Number to Insert", !self.text_mode, &theme).on_click(cx.listener(|this, _, _, cx| {
                        this.text_mode = false;
                        cx.notify();
                    })))
                    .child(
                        div()
                            .pl(px(20.0))
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(row("Initial number:", &self.initial, cx))
                            .child(row("Increase by:", &self.step, cx))
                            .child(row("Repeat:", &self.repeat, cx))
                            .child(
                                div()
                                    .flex()
                                    .gap(px(10.0))
                                    .child(ui::label("Leading:", &theme))
                                    .children(["None", "Zeros", "Spaces"].into_iter().enumerate().map(|(i, l)| {
                                        ui::radio(("ce-lead", i), l, leading == i as u8, &theme).on_click(cx.listener(move |this, _, _, cx| {
                                            this.leading = i as u8;
                                            cx.notify();
                                        }))
                                    })),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(10.0))
                                    .child(ui::label("Format:", &theme))
                                    .children(["Dec", "Hex", "Oct", "Bin"].into_iter().enumerate().map(|(i, l)| {
                                        ui::radio(("ce-fmt", i), l, format == i as u8, &theme).on_click(cx.listener(move |this, _, _, cx| {
                                            this.format = i as u8;
                                            cx.notify();
                                        }))
                                    })),
                            ),
                    ),
            )
            .child(
                button_row()
                    .child(ui::button("ce-cancel", "Cancel", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))))
                    .child(ui::primary_button("ce-ok", "OK", &theme).on_click(cx.listener(|this, _, _, cx| this.apply(cx)))),
            )
    }
}

// ====================================================================== Windows list

pub struct WindowsDialog {
    workspace: WeakEntity<Workspace>,
    selected: Option<(usize, usize)>,
    focus_handle: FocusHandle,
}
dialog_boilerplate!(WindowsDialog);

impl WindowsDialog {
    pub fn new(workspace: WeakEntity<Workspace>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        WindowsDialog { workspace, selected: None, focus_handle: cx.focus_handle() }
    }
}

impl Render for WindowsDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let Some(ws) = self.workspace.upgrade() else { return div().into_any_element() };
        let mut rows = Vec::new();
        for (pi, p) in ws.read(cx).panes.iter().enumerate() {
            for (i, e) in p.items.iter().enumerate() {
                let b = e.read(cx).buf(cx);
                let sel = self.selected == Some((pi, i));
                let size = b.len_chars();
                rows.push(
                    div()
                        .id(SharedString::from(format!("win-{pi}-{i}")))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(8.0))
                        .h(px(22.0))
                        .text_size(px(12.5))
                        .cursor_pointer()
                        .when(sel, |d| d.bg(theme.list_selected))
                        .hover(|d| d.bg(theme.list_hover))
                        .child(div().w(px(160.0)).overflow_hidden().whitespace_nowrap().child(format!("{}{}", b.display_name(), if b.is_dirty() { " *" } else { "" })))
                        .child(div().flex_1().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(theme.muted_fg).child(b.file.as_ref().and_then(|p| p.parent()).map(|p| p.to_string_lossy().to_string()).unwrap_or_default()))
                        .child(div().w(px(90.0)).text_color(theme.muted_fg).child(crate::syntax::display_name(&b.syntax_name)))
                        .child(div().w(px(70.0)).text_color(theme.muted_fg).child(super::chrome::fmt_num(size)))
                        .on_click(cx.listener(move |this, e: &gpui::ClickEvent, window, cx| {
                            this.selected = Some((pi, i));
                            if e.click_count() >= 2 {
                                if let Some(ws) = this.workspace.upgrade() {
                                    cx.emit(DismissEvent);
                                    ws.update(cx, |ws, cx| ws.activate(pi, i, window, cx));
                                }
                            }
                            cx.notify();
                        })),
                );
            }
        }
        ui::dialog_frame("Windows", 640.0, &theme)
            .track_focus(&self.focus_handle)
            .child(body().child(div().id("win-list").h(px(320.0)).overflow_y_scroll().border_1().border_color(theme.chrome_border).rounded(px(4.0)).children(rows)))
            .child(
                button_row()
                    .child(ui::button("win-sort", "Sort by Name", &theme).on_click(|_, window, cx| window.dispatch_action(Box::new(crate::actions::SortTabsByName), cx)))
                    .child(ui::button("win-close", "Close Document", &theme).on_click(cx.listener(|this, _, window, cx| {
                        if let (Some((pi, i)), Some(ws)) = (this.selected, this.workspace.upgrade()) {
                            this.selected = None;
                            ws.update(cx, |ws, cx| ws.close_tab_at(pi, i, window, cx));
                        }
                    })))
                    .child(ui::button("win-save", "Save", &theme).on_click(cx.listener(|this, _, window, cx| {
                        if let (Some((pi, i)), Some(ws)) = (this.selected, this.workspace.upgrade()) {
                            ws.update(cx, |ws, cx| {
                                if let Some(e) = ws.panes.get(pi).and_then(|p| p.items.get(i)).cloned() {
                                    ws.save_editor(e, super::files::SaveMode::Save, window, cx).detach();
                                }
                            });
                        }
                    })))
                    .child(ui::primary_button("win-activate", "Activate", &theme).on_click(cx.listener(|this, _, window, cx| {
                        if let (Some((pi, i)), Some(ws)) = (this.selected, this.workspace.upgrade()) {
                            cx.emit(DismissEvent);
                            ws.update(cx, |ws, cx| ws.activate(pi, i, window, cx));
                        }
                    }))),
            )
            .into_any_element()
    }
}

// ====================================================================== Find characters by code

pub struct CharRangeDialog {
    workspace: WeakEntity<Workspace>,
    mode: u8,
    from: Entity<Editor>,
    to: Entity<Editor>,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}
dialog_boilerplate!(CharRangeDialog);

impl CharRangeDialog {
    pub fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mk = |v: &str, window: &mut Window, cx: &mut Context<Self>| {
            let v = v.to_string();
            cx.new(|cx| {
                let mut e = Editor::single_line("", window, cx);
                e.set_text(&v, window, cx);
                e
            })
        };
        let from = mk("0", window, cx);
        let to = mk("255", window, cx);
        let subs = vec![dismiss_on_cancel(&from, window, cx), dismiss_on_cancel(&to, window, cx)];
        CharRangeDialog { workspace, mode: 0, from, to, focus_handle: cx.focus_handle(), _subs: subs }
    }

    fn range(&self, cx: &App) -> std::ops::RangeInclusive<u32> {
        match self.mode {
            0 => 128..=u32::MAX,
            1 => 0..=127,
            _ => {
                let a = self.from.read(cx).text(cx).trim().parse::<u32>().unwrap_or(0);
                let b = self.to.read(cx).text(cx).trim().parse::<u32>().unwrap_or(255);
                a.min(b)..=a.max(b)
            }
        }
    }
}

impl Render for CharRangeDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let mode = self.mode;
        let radio = |id: &'static str, label: &'static str, m: u8| {
            ui::radio(id, label, mode == m, &theme).on_click(cx.listener(move |this, _, _, cx| {
                this.mode = m;
                cx.notify();
            }))
        };
        ui::dialog_frame("Find Characters by Code", 420.0, &theme)
            .track_focus(&self.focus_handle)
            .child(
                body()
                    .child(radio("cr-non", "Non-ASCII Characters (128 and above)", 0))
                    .child(radio("cr-ascii", "ASCII Characters (0 - 127)", 1))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(radio("cr-my", "My range:", 2))
                            .child(ui::input(&self.from, &theme, cx).w(px(70.0)))
                            .child("–")
                            .child(ui::input(&self.to, &theme, cx).w(px(70.0))),
                    ),
            )
            .child(
                button_row()
                    .child(ui::button("cr-close", "Close", &theme).on_click(cx.listener(|_, _, _, cx| cx.emit(DismissEvent))))
                    .child(ui::button("cr-prev", "Find Previous", &theme).on_click(cx.listener(|this, _, _, cx| {
                        let r = this.range(cx);
                        if let Some(ws) = this.workspace.upgrade() {
                            ws.update(cx, |ws, cx| ws.find_char_range(r, false, cx));
                        }
                    })))
                    .child(ui::primary_button("cr-next", "Find Next", &theme).on_click(cx.listener(|this, _, _, cx| {
                        let r = this.range(cx);
                        if let Some(ws) = this.workspace.upgrade() {
                            ws.update(cx, |ws, cx| ws.find_char_range(r, true, cx));
                        }
                    }))),
            )
    }
}
