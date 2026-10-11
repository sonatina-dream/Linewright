//! Default key bindings (Windows-style editor shortcuts adapted to macOS) and user overrides
//! (Settings > Edit keymap.json).

use gpui::{App, KeyBinding};


use crate::editor::*;

pub const DOC: &str = "Editor && mode == full";
pub const COMPLETING: &str = "Editor && completing";
pub const COLUMN: &str = "Editor && mode == full && column_selecting";

pub fn editor_bindings() -> Vec<KeyBinding> {
    let e = Some("Editor");
    let d = Some(DOC);
    vec![
        // Movement
        KeyBinding::new("left", MoveLeft, e),
        KeyBinding::new("right", MoveRight, e),
        KeyBinding::new("up", MoveUp, e),
        KeyBinding::new("down", MoveDown, e),
        KeyBinding::new("ctrl-b", MoveLeft, e),
        KeyBinding::new("ctrl-f", MoveRight, e),
        KeyBinding::new("ctrl-p", MoveUp, e),
        KeyBinding::new("ctrl-n", MoveDown, e),
        KeyBinding::new("alt-left", MoveWordLeft, e),
        KeyBinding::new("alt-right", MoveWordRight, e),
        KeyBinding::new("cmd-left", MoveToLineStart, e),
        KeyBinding::new("cmd-right", MoveToLineEnd, e),
        KeyBinding::new("home", MoveToLineStart, e),
        KeyBinding::new("end", MoveToLineEnd, e),
        KeyBinding::new("ctrl-a", MoveToLineStart, e),
        KeyBinding::new("ctrl-e", MoveToLineEnd, e),
        KeyBinding::new("cmd-up", MoveToDocStart, e),
        KeyBinding::new("cmd-down", MoveToDocEnd, e),
        KeyBinding::new("ctrl-home", MoveToDocStart, e),
        KeyBinding::new("ctrl-end", MoveToDocEnd, e),
        KeyBinding::new("pageup", PageUp, e),
        KeyBinding::new("pagedown", PageDown, e),
        KeyBinding::new("shift-left", SelectLeft, e),
        KeyBinding::new("shift-right", SelectRight, e),
        KeyBinding::new("shift-up", SelectUp, e),
        KeyBinding::new("shift-down", SelectDown, e),
        KeyBinding::new("alt-shift-left", SelectWordLeft, e),
        KeyBinding::new("alt-shift-right", SelectWordRight, e),
        KeyBinding::new("cmd-shift-left", SelectToLineStart, e),
        KeyBinding::new("cmd-shift-right", SelectToLineEnd, e),
        KeyBinding::new("shift-home", SelectToLineStart, e),
        KeyBinding::new("shift-end", SelectToLineEnd, e),
        KeyBinding::new("cmd-shift-up", SelectToDocStart, e),
        KeyBinding::new("cmd-shift-down", SelectToDocEnd, e),
        KeyBinding::new("ctrl-shift-home", SelectToDocStart, e),
        KeyBinding::new("ctrl-shift-end", SelectToDocEnd, e),
        KeyBinding::new("shift-pageup", SelectPageUp, e),
        KeyBinding::new("shift-pagedown", SelectPageDown, e),
        // Editing
        KeyBinding::new("backspace", Backspace, e),
        KeyBinding::new("shift-backspace", Backspace, e),
        KeyBinding::new("ctrl-h", Backspace, e),
        KeyBinding::new("delete", Delete, e),
        KeyBinding::new("ctrl-d", Delete, e),
        KeyBinding::new("alt-backspace", DeleteWordLeft, e),
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, e),
        KeyBinding::new("alt-delete", DeleteWordRight, e),
        KeyBinding::new("ctrl-delete", DeleteWordRight, e),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, e),
        KeyBinding::new("cmd-delete", DeleteToLineEnd, e),
        KeyBinding::new("ctrl-k", DeleteToLineEnd, e),
        KeyBinding::new("enter", Newline, e),
        KeyBinding::new("shift-enter", Newline, e),
        KeyBinding::new("tab", Tab, e),
        KeyBinding::new("shift-tab", ShiftTab, e),
        KeyBinding::new("escape", Cancel, e),
        KeyBinding::new("cmd-z", Undo, e),
        KeyBinding::new("cmd-shift-z", Redo, e),
        KeyBinding::new("cmd-y", Redo, e),
        KeyBinding::new("cmd-x", Cut, e),
        KeyBinding::new("cmd-c", Copy, e),
        KeyBinding::new("cmd-v", Paste, e),
        KeyBinding::new("cmd-a", SelectAll, e),
        // Document editor only
        KeyBinding::new("alt-shift-up", ColumnSelectUp, d),
        KeyBinding::new("alt-shift-down", ColumnSelectDown, d),
        KeyBinding::new("alt-shift-left", ColumnSelectLeft, Some(COLUMN)),
        KeyBinding::new("alt-shift-right", ColumnSelectRight, Some(COLUMN)),
        KeyBinding::new("ctrl-alt-shift-left", ColumnSelectLeft, d),
        KeyBinding::new("ctrl-alt-shift-right", ColumnSelectRight, d),
        KeyBinding::new("cmd-alt-up", AddCursorAbove, d),
        KeyBinding::new("cmd-alt-down", AddCursorBelow, d),
        KeyBinding::new("cmd-shift-d", SelectNextOccurrence, d),
        KeyBinding::new("cmd-shift-enter", NewlineAbove, d),
        KeyBinding::new("cmd-enter", NewlineBelow, d),
        KeyBinding::new("cmd-]", Indent, d),
        KeyBinding::new("cmd-[", Outdent, d),
        KeyBinding::new("cmd-d", DuplicateLine, d),
        KeyBinding::new("cmd-shift-l", DeleteLine, d),
        KeyBinding::new("cmd-shift-k", DeleteLine, d),
        KeyBinding::new("ctrl-shift-up", MoveLineUp, d),
        KeyBinding::new("ctrl-shift-down", MoveLineDown, d),
        KeyBinding::new("cmd-ctrl-up", MoveLineUp, d),
        KeyBinding::new("cmd-ctrl-down", MoveLineDown, d),
        KeyBinding::new("cmd-j", JoinLines, d),
        KeyBinding::new("cmd-i", SplitLines, d),
        KeyBinding::new("ctrl-t", TransposeLines, d),
        KeyBinding::new("insert", ToggleOverwrite, d),
        KeyBinding::new("cmd-u", LowerCase, d),
        KeyBinding::new("cmd-shift-u", UpperCase, d),
        KeyBinding::new("cmd-/", ToggleLineComment, d),
        KeyBinding::new("cmd-alt-/", BlockComment, d),
        KeyBinding::new("cmd-b", GoToMatchingBrace, d),
        KeyBinding::new("cmd-shift-b", SelectToMatchingBrace, d),
        KeyBinding::new("cmd-f2", ToggleBookmark, d),
        KeyBinding::new("f2", NextBookmark, d),
        KeyBinding::new("shift-f2", PrevBookmark, d),
        KeyBinding::new("ctrl-alt-f", FoldCurrent, d),
        KeyBinding::new("ctrl-alt-shift-f", UnfoldCurrent, d),
        KeyBinding::new("alt-0", FoldAll, d),
        KeyBinding::new("alt-shift-0", UnfoldAll, d),
        KeyBinding::new("alt-1", FoldLevel1, d),
        KeyBinding::new("alt-2", FoldLevel2, d),
        KeyBinding::new("alt-3", FoldLevel3, d),
        KeyBinding::new("alt-4", FoldLevel4, d),
        KeyBinding::new("alt-5", FoldLevel5, d),
        KeyBinding::new("alt-6", FoldLevel6, d),
        KeyBinding::new("alt-7", FoldLevel7, d),
        KeyBinding::new("alt-8", FoldLevel8, d),
        KeyBinding::new("alt-shift-1", UnfoldLevel1, d),
        KeyBinding::new("alt-shift-2", UnfoldLevel2, d),
        KeyBinding::new("alt-shift-3", UnfoldLevel3, d),
        KeyBinding::new("alt-shift-4", UnfoldLevel4, d),
        KeyBinding::new("alt-shift-5", UnfoldLevel5, d),
        KeyBinding::new("alt-shift-6", UnfoldLevel6, d),
        KeyBinding::new("alt-shift-7", UnfoldLevel7, d),
        KeyBinding::new("alt-shift-8", UnfoldLevel8, d),
        KeyBinding::new("ctrl-space", TriggerCompletion, d),
        KeyBinding::new("ctrl-enter", TriggerCompletion, d),
        // Completion popup
        KeyBinding::new("down", CompletionNext, Some(COMPLETING)),
        KeyBinding::new("up", CompletionPrev, Some(COMPLETING)),
        KeyBinding::new("ctrl-n", CompletionNext, Some(COMPLETING)),
        KeyBinding::new("ctrl-p", CompletionPrev, Some(COMPLETING)),
        KeyBinding::new("enter", CompletionConfirm, Some(COMPLETING)),
        KeyBinding::new("tab", CompletionConfirm, Some(COMPLETING)),
    ]
}

pub fn workspace_bindings() -> Vec<KeyBinding> {
    use crate::actions::*;
    let w = Some("Workspace");
    vec![
        KeyBinding::new("cmd-n", NewFile, w),
        KeyBinding::new("cmd-t", NewFile, w),
        KeyBinding::new("cmd-o", OpenFile, w),
        KeyBinding::new("cmd-s", Save, w),
        KeyBinding::new("cmd-shift-s", SaveAs, w),
        KeyBinding::new("cmd-alt-s", SaveAll, w),
        KeyBinding::new("cmd-w", CloseTab, w),
        KeyBinding::new("cmd-shift-w", CloseAll, w),
        KeyBinding::new("cmd-shift-t", ReopenClosedTab, w),
        KeyBinding::new("cmd-r", ReloadFromDisk, w),
        KeyBinding::new("cmd-p", Print, w),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", HideApp, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-,", OpenPreferences, w),
        // Search
        KeyBinding::new("cmd-f", Find, w),
        KeyBinding::new("cmd-alt-f", Replace, w),
        KeyBinding::new("cmd-shift-f", FindInFiles, w),
        KeyBinding::new("cmd-shift-m", MarkDialog, w),
        KeyBinding::new("cmd-g", FindNext, w),
        KeyBinding::new("cmd-shift-g", FindPrev, w),
        KeyBinding::new("f3", FindNext, w),
        KeyBinding::new("shift-f3", FindPrev, w),
        KeyBinding::new("cmd-f3", SelectAndFindNext, w),
        KeyBinding::new("cmd-shift-f3", SelectAndFindPrev, w),
        KeyBinding::new("cmd-alt-f3", FindWordNext, w),
        KeyBinding::new("cmd-alt-shift-f3", FindWordPrev, w),
        KeyBinding::new("cmd-e", UseSelectionForFind, w),
        KeyBinding::new("cmd-alt-i", IncrementalSearch, w),
        KeyBinding::new("cmd-l", GoToLine, w),
        KeyBinding::new("ctrl-g", GoToLine, w),
        KeyBinding::new("f4", NextSearchResult, w),
        KeyBinding::new("shift-f4", PrevSearchResult, w),
        KeyBinding::new("f7", ToggleSearchResultsPanel, w),
        KeyBinding::new("escape", crate::editor::Cancel, w),
        // View
        KeyBinding::new("cmd-=", ZoomIn, w),
        KeyBinding::new("cmd-+", ZoomIn, w),
        KeyBinding::new("cmd--", ZoomOut, w),
        KeyBinding::new("cmd-0", ZoomReset, w),
        KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, w),
        KeyBinding::new("f11", ToggleFullScreen, w),
        KeyBinding::new("f12", ToggleFloatingNote, w),
        KeyBinding::new("alt-z", ToggleWordWrap, w),
        KeyBinding::new("ctrl-tab", NextTab, w),
        KeyBinding::new("ctrl-shift-tab", PrevTab, w),
        KeyBinding::new("cmd-shift-]", NextTab, w),
        KeyBinding::new("cmd-shift-[", PrevTab, w),
        KeyBinding::new("cmd-alt-pagedown", MoveTabForward, w),
        KeyBinding::new("cmd-alt-pageup", MoveTabBackward, w),
        KeyBinding::new("cmd-1", GoToTab1, w),
        KeyBinding::new("cmd-2", GoToTab2, w),
        KeyBinding::new("cmd-3", GoToTab3, w),
        KeyBinding::new("cmd-4", GoToTab4, w),
        KeyBinding::new("cmd-5", GoToTab5, w),
        KeyBinding::new("cmd-6", GoToTab6, w),
        KeyBinding::new("cmd-7", GoToTab7, w),
        KeyBinding::new("cmd-8", GoToTab8, w),
        KeyBinding::new("cmd-9", GoToLastTab, w),
        KeyBinding::new("f8", FocusOtherView, w),
        // Macro & run
        KeyBinding::new("ctrl-shift-r", ToggleRecording, w),
        KeyBinding::new("ctrl-shift-p", PlaybackMacro, w),
        KeyBinding::new("f5", RunCommand, w),
        // Tools
        KeyBinding::new("cmd-alt-c", ColumnEditor, w),
    ]
}

pub fn user_keymap_path() -> std::path::PathBuf {
    crate::settings::config_dir().join("keymap.json")
}

pub const USER_KEYMAP_TEMPLATE: &str = r#"{
  "//": "Custom keyboard shortcuts for Linewright. Map a keystroke to an action name.",
  "//2": "Examples (remove the leading underscore to enable). See Settings > Keyboard Shortcuts for all actions.",
  "_cmd-shift-d": "editor::DuplicateLine",
  "_ctrl-q": "editor::ToggleLineComment",
  "_cmd-k": "workspace::ToggleFunctionList"
}
"#;

/// Load `keymap.json`: an object mapping keystrokes to action names. Returns the number of
/// bindings loaded and any errors.
pub fn load_user_keymap(cx: &mut App) -> (usize, Vec<String>) {
    let path = user_keymap_path();
    let Ok(text) = std::fs::read_to_string(&path) else { return (0, Vec::new()) };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return (0, vec![format!("keymap.json: {e}")]),
    };
    let Some(map) = value.as_object() else { return (0, vec!["keymap.json must be a JSON object".into()]) };
    let mut bindings = Vec::new();
    let mut errors = Vec::new();
    for (keys, action) in map {
        if keys.starts_with('_') || keys.starts_with("//") {
            continue;
        }
        let Some(name) = action.as_str() else { continue };
        let action = match cx.build_action(name, None) {
            Ok(a) => a,
            Err(e) => {
                errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        let context = if name.starts_with("editor::") { DOC } else { "Workspace" };
        let predicate = gpui::KeyBindingContextPredicate::parse(context).ok().map(std::rc::Rc::new);
        match KeyBinding::load(keys, action, predicate, false, None, &gpui::DummyKeyboardMapper) {
            Ok(b) => bindings.push(b),
            Err(e) => errors.push(format!("{keys}: {e:?}")),
        }
    }
    let n = bindings.len();
    cx.bind_keys(bindings);
    (n, errors)
}

/// Rebuild the whole keymap (defaults + user overrides).
pub fn reload_keymap(cx: &mut App) -> (usize, Vec<String>) {
    cx.clear_key_bindings();
    cx.bind_keys(editor_bindings());
    cx.bind_keys(workspace_bindings());
    load_user_keymap(cx)
}
