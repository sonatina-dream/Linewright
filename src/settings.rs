//! User preferences (Settings > Preferences), persisted as JSON in the app config directory.

use std::path::PathBuf;

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // Appearance
    pub theme: String,
    pub font_family: String,
    pub font_size: f32,
    pub ui_font_size: f32,
    pub zoom: i32,
    pub show_toolbar: bool,
    pub show_status_bar: bool,
    pub show_tab_bar: bool,
    pub tab_close_button: bool,
    pub double_click_closes_tab: bool,
    pub always_on_top: bool,

    // Editing
    pub tab_size: usize,
    pub use_spaces: bool,
    pub auto_indent: bool,
    pub word_wrap: bool,
    pub show_whitespace: bool,
    pub show_eol: bool,
    pub show_indent_guides: bool,
    pub show_wrap_symbol: bool,
    pub show_line_numbers: bool,
    pub show_bookmark_margin: bool,
    pub show_fold_margin: bool,
    pub show_change_history: bool,
    pub highlight_current_line: bool,
    pub current_line_frame: bool,
    pub caret_width: f32,
    pub caret_blink: bool,
    pub edge_column: usize,
    pub show_edge: bool,
    pub multi_editing: bool,
    pub scroll_beyond_last_line: bool,
    pub smooth_scroll: bool,
    pub copy_line_without_selection: bool,

    // Highlighting
    pub smart_highlight: bool,
    pub smart_highlight_match_case: bool,
    pub smart_highlight_whole_word: bool,
    pub brace_highlight: bool,
    pub match_tag_highlight: bool,

    // Auto-completion / auto-insert
    pub auto_completion: bool,
    pub auto_completion_min_chars: usize,
    pub auto_close_brackets: bool,
    pub auto_close_quotes: bool,
    pub auto_close_html_tags: bool,

    // New document
    pub new_doc_eol: String,
    pub new_doc_encoding: String,
    pub new_doc_language: String,

    // Files & session
    pub remember_session: bool,
    pub session_snapshot: bool,
    pub backup_interval_secs: u64,
    pub file_change_detection: bool,
    pub auto_reload_unmodified: bool,
    pub max_recent_files: usize,
    pub default_directory: String,
    pub trim_trailing_on_save: bool,
    pub ensure_final_newline: bool,

    // Search
    pub search_engine: String,
    pub find_dialog_fill_from_selection: bool,
    pub stop_find_on_wrap: bool,

    // Misc
    pub date_time_format: String,
    pub split_vertical: bool,
    pub left_panel_width: f32,
    pub right_panel_width: f32,
    pub bottom_panel_height: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: crate::theme::DEFAULT_THEME.into(),
            font_family: "Menlo".into(),
            font_size: 13.0,
            ui_font_size: 13.0,
            zoom: 0,
            show_toolbar: true,
            show_status_bar: true,
            show_tab_bar: true,
            tab_close_button: true,
            double_click_closes_tab: false,
            always_on_top: false,
            tab_size: 4,
            use_spaces: false,
            auto_indent: true,
            word_wrap: false,
            show_whitespace: false,
            show_eol: false,
            show_indent_guides: true,
            show_wrap_symbol: false,
            show_line_numbers: true,
            show_bookmark_margin: true,
            show_fold_margin: true,
            show_change_history: true,
            highlight_current_line: true,
            current_line_frame: false,
            caret_width: 1.5,
            caret_blink: true,
            edge_column: 80,
            show_edge: false,
            multi_editing: true,
            scroll_beyond_last_line: true,
            smooth_scroll: true,
            copy_line_without_selection: true,
            smart_highlight: true,
            smart_highlight_match_case: false,
            smart_highlight_whole_word: true,
            brace_highlight: true,
            match_tag_highlight: true,
            auto_completion: true,
            auto_completion_min_chars: 3,
            auto_close_brackets: false,
            auto_close_quotes: false,
            auto_close_html_tags: false,
            new_doc_eol: "lf".into(),
            new_doc_encoding: "utf-8".into(),
            new_doc_language: crate::syntax::PLAIN_TEXT.into(),
            remember_session: true,
            session_snapshot: true,
            backup_interval_secs: 7,
            file_change_detection: true,
            auto_reload_unmodified: false,
            max_recent_files: 15,
            default_directory: String::new(),
            trim_trailing_on_save: false,
            ensure_final_newline: false,
            search_engine: "https://www.google.com/search?q=$(CURRENT_WORD)".into(),
            find_dialog_fill_from_selection: true,
            stop_find_on_wrap: false,
            date_time_format: "%H:%M %Y-%m-%d".into(),
            split_vertical: true,
            left_panel_width: 240.0,
            right_panel_width: 260.0,
            bottom_panel_height: 220.0,
        }
    }
}

impl Global for Settings {}

pub fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("LINEWRIGHT_CONFIG_DIR") {
        let dir = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }
    let base = dirs::config_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));
    let dir = base.join(crate::APP_NAME);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn backup_dir() -> PathBuf {
    let dir = config_dir().join("backup");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn settings_path() -> PathBuf {
    config_dir().join("config.json")
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(settings_path(), json);
        }
    }

    pub fn effective_font_size(&self) -> f32 {
        (self.font_size + self.zoom as f32).clamp(5.0, 72.0)
    }
}

pub fn update_settings(cx: &mut App, f: impl FnOnce(&mut Settings)) {
    let mut s = cx.global::<Settings>().clone();
    f(&mut s);
    s.save();
    cx.set_global(s);
}

/// Change settings without persisting them (tests, transient toggles).
pub fn update_settings_in_memory(cx: &mut App, f: impl FnOnce(&mut Settings)) {
    let mut s = cx.global::<Settings>().clone();
    f(&mut s);
    cx.set_global(s);
}
