//! Session persistence (open tabs, carets, bookmarks, unsaved-document snapshots) and other
//! long-lived application state (recent files, find history, saved macros).

use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

/// Set for secondary instances (File > New Instance) so they don't overwrite the main session.
pub static SESSION_DISABLED: AtomicBool = AtomicBool::new(false);

pub fn session_enabled() -> bool {
    !SESSION_DISABLED.load(Ordering::Relaxed)
}

use crate::settings::{backup_dir, config_dir};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionFile {
    pub path: Option<String>,
    pub untitled_name: Option<String>,
    /// Snapshot of unsaved content (session snapshot / periodic backup).
    pub backup: Option<String>,
    pub dirty: bool,
    pub anchor: usize,
    pub caret: usize,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub language: String,
    pub encoding: String,
    pub eol: String,
    pub read_only: bool,
    pub bookmarks: Vec<usize>,
    pub folds: Vec<(usize, usize)>,
    pub monitoring: bool,
    pub pinned: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionPane {
    pub files: Vec<SessionFile>,
    pub active: usize,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub panes: Vec<SessionPane>,
    pub active_pane: usize,
    pub window: Option<(f32, f32, f32, f32)>,
    pub folder_roots: Vec<String>,
    pub left_panel: Option<String>,
    pub right_panel: Option<String>,
    pub split_ratio: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SavedStep {
    Action(String),
    Insert(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedMacro {
    pub name: String,
    pub steps: Vec<SavedStep>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub recent_files: Vec<String>,
    pub find_history: Vec<String>,
    pub replace_history: Vec<String>,
    pub find_dirs: Vec<String>,
    pub find_filters: Vec<String>,
    pub run_history: Vec<String>,
    pub macros: Vec<SavedMacro>,
    pub untitled_counter: usize,
}

fn session_path() -> PathBuf {
    config_dir().join("session.json")
}

fn state_path() -> PathBuf {
    config_dir().join("state.json")
}

impl Session {
    pub fn load() -> Option<Session> {
        Self::load_from(&session_path())
    }

    pub fn load_from(path: &Path) -> Option<Session> {
        let s = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save(&self) {
        if session_enabled() {
            self.save_to(&session_path());
        }
    }

    pub fn save_to(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

impl AppState {
    pub fn load() -> AppState {
        std::fs::read_to_string(state_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(state_path(), json);
        }
    }

    pub fn push_recent(&mut self, path: &Path, max: usize) {
        let p = path.to_string_lossy().to_string();
        self.recent_files.retain(|r| *r != p);
        self.recent_files.insert(0, p);
        self.recent_files.truncate(max.max(1));
    }

    pub fn push_history(list: &mut Vec<String>, item: &str) {
        if item.is_empty() {
            return;
        }
        list.retain(|r| r != item);
        list.insert(0, item.to_string());
        list.truncate(30);
    }
}

/// File name for a snapshot backup: `name@YYYY-MM-DD_HHMMSS_<buffer id>`.
pub fn new_backup_path(display_name: &str, buffer_id: u64) -> PathBuf {
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
    let safe: String = display_name
        .chars()
        .map(|c| if c == '/' || c == ':' || c == '\\' { '_' } else { c })
        .collect();
    backup_dir().join(format!("{safe}@{stamp}_{buffer_id}"))
}

