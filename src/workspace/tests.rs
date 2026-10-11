//! Headless end-to-end tests of the workspace (menus/actions, find dialog, tabs, views, macros, files).

use std::path::PathBuf;

use gpui::{AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};

use super::find_panel::*;
use super::Workspace;
use crate::actions::*;
use crate::buffer::Selection;
use crate::editor::{self, Editor};
use crate::settings::Settings;

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("linewright-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn init(cx: &mut TestAppContext) {
    let cfg = std::env::temp_dir().join(format!("linewright-test-config-{}", std::process::id()));
    // SAFETY: tests only ever set the same value.
    unsafe { std::env::set_var("LINEWRIGHT_CONFIG_DIR", &cfg) };
    cx.update(|cx| {
        cx.set_global(Settings { session_snapshot: false, remember_session: false, ..Default::default() });
        crate::theme::init(cx, crate::theme::DEFAULT_THEME);
        cx.set_global(editor::MacroRecorder::default());
        cx.set_global(editor::ClipboardHistory::default());
        crate::keymap::reload_keymap(cx);
    });
}

fn workspace(cx: &mut TestAppContext) -> (Entity<Workspace>, &mut VisualTestContext) {
    init(cx);
    let (ws, cx) = cx.add_window_view(|window, cx| {
        let mut ws = Workspace::new(window, cx);
        ws.new_file(&NewFile, window, cx);
        ws
    });
    cx.run_until_parked();
    (ws, cx)
}

fn active(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<Editor> {
    cx.update(|_, cx| ws.read(cx).active_editor().unwrap())
}

fn text(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    let e = active(ws, cx);
    cx.update(|_, cx| e.read(cx).text(cx))
}

fn set_find(ws: &Entity<Workspace>, cx: &mut VisualTestContext, pattern: &str, replace: &str, f: impl FnOnce(&mut FindPanel)) {
    let (pattern, replace) = (pattern.to_string(), replace.to_string());
    cx.update(|window, cx| {
        let panel = ws.read(cx).find_panel.clone();
        panel.update(cx, |p, cx| {
            p.find_input.update(cx, |e, cx| e.set_text(&pattern, window, cx));
            p.replace_input.update(cx, |e, cx| e.set_text(&replace, window, cx));
            f(p);
        });
    });
}

fn status(ws: &Entity<Workspace>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| ws.read(cx).find_panel.read(cx).status.to_string())
}

fn focus_doc(ws: &Entity<Workspace>, cx: &mut VisualTestContext) {
    let e = active(ws, cx);
    cx.update(|window, cx| window.focus(&e.focus_handle(cx)));
}

#[gpui::test]
fn type_and_dirty(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("hello world");
    assert_eq!(text(&ws, cx), "hello world");
    let e = active(&ws, cx);
    assert!(cx.update(|_, cx| e.read(cx).buf(cx).is_dirty()));
    assert_eq!(cx.update(|_, cx| e.read(cx).buf(cx).display_name()), "new 1");
}

#[gpui::test]
fn find_next_prev_count_replace(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("one two one three ONE");
    cx.simulate_keystrokes("cmd-up");
    set_find(&ws, cx, "one", "1", |_| {});
    cx.dispatch_action(FindNext);
    let e = active(&ws, cx);
    let sel = |cx: &mut VisualTestContext| cx.update(|_, cx| e.read(cx).primary_selection().range());
    assert_eq!(sel(cx), 0..3);
    cx.dispatch_action(FindNext);
    assert_eq!(sel(cx), 8..11);
    cx.dispatch_action(FindNext);
    assert_eq!(sel(cx), 18..21); // case-insensitive by default
    cx.dispatch_action(FindNext);
    assert_eq!(sel(cx), 0..3); // wrapped
    assert!(status(&ws, cx).contains("continued from the top"));
    cx.dispatch_action(FindPrev);
    assert_eq!(sel(cx), 18..21);
    cx.dispatch_action(FpCount);
    assert_eq!(status(&ws, cx), "3 matches in the whole document.");
    set_find(&ws, cx, "one", "1", |p| p.opts.match_case = true);
    cx.dispatch_action(FpReplaceAll);
    assert_eq!(text(&ws, cx), "1 two 1 three ONE");
    // One undo step reverts the whole Replace All.
    focus_doc(&ws, cx);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&ws, cx), "one two one three ONE");
}

#[gpui::test]
fn regex_and_extended_replace(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("john smith\njane doe");
    set_find(&ws, cx, r"^(\w+) (\w+)$", r"\U$2\E, $1", |p| p.opts.mode = crate::search::SearchMode::Regex);
    cx.dispatch_action(FpReplaceAll);
    assert_eq!(text(&ws, cx), "SMITH, john\nDOE, jane");
    set_find(&ws, cx, r"\n", r"\t", |p| p.opts.mode = crate::search::SearchMode::Extended);
    cx.dispatch_action(FpReplaceAll);
    assert_eq!(text(&ws, cx), "SMITH, john\tDOE, jane");
}

#[gpui::test]
fn find_all_and_results_navigation(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("alpha\nbeta alpha\ngamma\nalpha");
    set_find(&ws, cx, "alpha", "", |_| {});
    cx.dispatch_action(FpFindAllCurrent);
    let (visible, total, files) = cx.update(|_, cx| {
        let r = ws.read(cx).results.read(cx);
        (ws.read(cx).results_visible, r.sets[0].total, r.sets[0].files[0].hits.len())
    });
    assert!(visible);
    assert_eq!(total, 3);
    assert_eq!(files, 3);
    cx.dispatch_action(NextSearchResult);
    cx.run_until_parked();
    let e = active(&ws, cx);
    assert_eq!(cx.update(|_, cx| e.read(cx).primary_selection().range()), 0..5);
    cx.dispatch_action(NextSearchResult);
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| e.read(cx).primary_selection().range()), 11..16);
}

#[gpui::test]
fn mark_all_and_bookmarks(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("x\nfoo\nx foo\n");
    set_find(&ws, cx, "foo", "", |p| p.bookmark_line = true);
    cx.dispatch_action(FpMarkAll);
    let e = active(&ws, cx);
    let (marks, bms) = cx.update(|_, cx| {
        let b = e.read(cx).buf(cx);
        (b.marks.iter().filter(|m| m.style == 0).count(), b.bookmarks.iter().copied().collect::<Vec<_>>())
    });
    assert_eq!(marks, 2);
    assert_eq!(bms, vec![1, 2]);
    focus_doc(&ws, cx);
    cx.dispatch_action(editor::KeepOnlyBookmarkedLines);
    assert_eq!(text(&ws, cx), "foo\nx foo\n");
    cx.dispatch_action(FpClearMarks);
    assert_eq!(cx.update(|_, cx| e.read(cx).buf(cx).marks.len()), 0);
}

#[gpui::test]
fn tabs_views_and_clone(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("first");
    cx.dispatch_action(NewFile);
    cx.simulate_input("second");
    cx.dispatch_action(NewFile);
    assert_eq!(cx.update(|_, cx| ws.read(cx).panes[0].items.len()), 3);
    cx.dispatch_action(NextTab);
    assert_eq!(text(&ws, cx), "first");
    cx.dispatch_action(GoToTab2);
    assert_eq!(text(&ws, cx), "second");
    // Clone to the other view shares the buffer.
    cx.dispatch_action(CloneToOtherView);
    assert!(cx.update(|_, cx| ws.read(cx).is_split()));
    assert_eq!(cx.update(|_, cx| ws.read(cx).active_pane), 1);
    cx.simulate_keystrokes("cmd-down");
    cx.simulate_input("!");
    cx.dispatch_action(FocusOtherView);
    assert_eq!(cx.update(|_, cx| ws.read(cx).active_pane), 0);
    assert_eq!(text(&ws, cx), "second!");
    // Close the empty third tab without prompting.
    cx.dispatch_action(GoToTab3);
    cx.dispatch_action(CloseTab);
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| ws.read(cx).panes[0].items.len()), 2);
    // Move a document to the other view.
    cx.dispatch_action(GoToTab1);
    cx.dispatch_action(MoveToOtherView);
    assert_eq!(cx.update(|_, cx| (ws.read(cx).panes[0].items.len(), ws.read(cx).panes[1].items.len())), (1, 2));
}

#[gpui::test]
fn close_dirty_prompts(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("unsaved");
    cx.dispatch_action(NewFile);
    cx.dispatch_action(GoToTab1);
    cx.dispatch_action(CloseTab);
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Don't Save");
    cx.run_until_parked();
    assert_eq!(cx.update(|_, cx| ws.read(cx).panes[0].items.len()), 1);
    assert_eq!(text(&ws, cx), "");
}

#[gpui::test]
fn macro_record_and_playback(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("a\nb\nc");
    cx.simulate_keystrokes("cmd-up");
    cx.dispatch_action(StartRecording);
    cx.simulate_input("- ");
    cx.simulate_keystrokes("down cmd-left");
    cx.dispatch_action(StopRecording);
    assert_eq!(text(&ws, cx), "- a\nb\nc");
    cx.dispatch_action(PlaybackMacro);
    assert_eq!(text(&ws, cx), "- a\n- b\nc");
    cx.update(|window, cx| ws.update(cx, |ws, cx| ws.run_macro_choice(0, 0, window, cx)));
    assert_eq!(text(&ws, cx), "- a\n- b\n- c");
}

#[gpui::test]
fn save_as_and_reopen(cx: &mut TestAppContext) {
    let dir = temp_dir("save");
    let (ws, cx) = workspace(cx);
    cx.simulate_input("saved text\n");
    let target = dir.join("out.txt");
    let t2 = target.clone();
    cx.dispatch_action(Save);
    cx.run_until_parked();
    cx.simulate_new_path_selection(move |_| Some(t2));
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "saved text\n");
    let e = active(&ws, cx);
    assert!(!cx.update(|_, cx| e.read(cx).buf(cx).is_dirty()));
    assert_eq!(cx.update(|_, cx| e.read(cx).buf(cx).display_name()), "out.txt");
    // Convert to UTF-16 LE BOM and save again.
    cx.dispatch_action(ConvertToUtf16Le);
    cx.dispatch_action(Save);
    cx.run_until_parked();
    let bytes = std::fs::read(&target).unwrap();
    assert_eq!(&bytes[..4], &[0xFF, 0xFE, b's', 0]);
    // Opening it again activates the existing tab instead of duplicating it.
    cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open_paths(vec![target.clone()], window, cx)));
    assert_eq!(cx.update(|_, cx| ws.read(cx).panes[0].items.len()), 1);
}

#[gpui::test]
fn open_file_detects_language_and_eol(cx: &mut TestAppContext) {
    let dir = temp_dir("open");
    let p = dir.join("main.py");
    std::fs::write(&p, "def f():\r\n    return 1\r\n").unwrap();
    let (ws, cx) = workspace(cx);
    cx.update(|window, cx| ws.update(cx, |ws, cx| ws.open_paths(vec![p.clone()], window, cx)));
    // The untouched "new 1" was replaced.
    assert_eq!(cx.update(|_, cx| ws.read(cx).panes[0].items.len()), 1);
    let e = active(&ws, cx);
    let (lang, eol) = cx.update(|_, cx| {
        let b = e.read(cx).buf(cx);
        (b.syntax_name.clone(), b.eol)
    });
    assert_eq!(lang, "Python");
    assert_eq!(eol, crate::encoding::EolMode::Crlf);
    cx.dispatch_action(EolUnix);
    assert_eq!(text(&ws, cx), "def f():\n    return 1\n");
    cx.dispatch_action(SetLanguage { name: "Rust".into() });
    assert_eq!(cx.update(|_, cx| e.read(cx).buf(cx).syntax_name.clone()), "Rust");
}

#[gpui::test]
fn menu_commands_reach_document_from_find_input(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("make me loud");
    cx.simulate_keystrokes("cmd-a");
    cx.dispatch_action(Find);
    // Focus is in the find box now; Edit > Convert Case must still hit the document.
    let focused_find = cx.update(|window, cx| {
        let p = ws.read(cx).find_panel.read(cx).find_input.clone();
        p.focus_handle(cx).is_focused(window)
    });
    assert!(focused_find);
    cx.dispatch_action(editor::UpperCase);
    assert_eq!(text(&ws, cx), "MAKE ME LOUD");
    cx.simulate_keystrokes("escape");
    assert!(!cx.update(|_, cx| ws.read(cx).find_visible));
}

#[gpui::test]
fn compare_views(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("a\nb\nc");
    cx.dispatch_action(NewFile);
    cx.simulate_input("a\nX\nc\nd");
    cx.dispatch_action(CompareWithOtherView);
    let (left, right) = cx.update(|_, cx| {
        let w = ws.read(cx);
        let l = w.panes[0].active_item().unwrap().read(cx).buf(cx).marks.iter().filter(|m| m.style >= 6).count();
        let r = w.panes[1].active_item().unwrap().read(cx).buf(cx).marks.iter().filter(|m| m.style >= 6).count();
        (l, r)
    });
    // The previous tab moved to the right view: left = "a X c d", right = "a b c".
    assert_eq!(left, 2);
    assert_eq!(right, 1);
}

#[gpui::test]
fn column_editor_numbers(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("a\nb\nc");
    cx.simulate_keystrokes("cmd-up");
    cx.update(|_, cx| {
        ws.update(cx, |ws, cx| ws.insert_column(super::commands::ColumnInsert::Numbers(Box::new(|k| format!("{}. ", k + 1))), cx))
    });
    assert_eq!(text(&ws, cx), "1. a\n2. b\n3. c");
}

#[gpui::test]
fn session_snapshot_roundtrip(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.update(|_, cx| crate::settings::update_settings_in_memory(cx, |s| s.session_snapshot = true));
    cx.simulate_input("unsaved draft");
    let session = cx.update(|_, cx| {
        ws.update(cx, |ws, cx| ws.write_backups(cx));
        ws.read(cx).session_snapshot(cx)
    });
    cx.run_until_parked();
    let backup = session.panes[0].files[0].backup.clone().expect("backup written");
    assert!(std::path::Path::new(&backup).exists());
    // Restore into a fresh workspace.
    let ws2 = cx.update(|window, cx| {
        cx.new(|cx| {
            let mut w = Workspace::new(window, cx);
            w.restore_session(&session, window, cx);
            w
        })
    });
    let restored = cx.update(|_, cx| ws2.read(cx).active_editor().unwrap().read(cx).text(cx));
    assert_eq!(restored, "unsaved draft");
    let dirty = cx.update(|_, cx| ws2.read(cx).active_editor().unwrap().read(cx).buf(cx).is_dirty());
    assert!(dirty);
}

#[gpui::test]
fn find_in_files_background(cx: &mut TestAppContext) {
    let dir = temp_dir("fif");
    std::fs::write(dir.join("a.txt"), "needle here\nno\nneedle again").unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/b.rs"), "fn needle() {}").unwrap();
    std::fs::write(dir.join("skip.bin"), b"needle\0binary").unwrap();
    let (ws, cx) = workspace(cx);
    let d = dir.to_string_lossy().to_string();
    cx.update(|window, cx| {
        let panel = ws.read(cx).find_panel.clone();
        panel.update(cx, |p, cx| {
            p.find_input.update(cx, |e, cx| e.set_text("needle", window, cx));
            p.dir_input.update(cx, |e, cx| e.set_text(&d, window, cx));
            p.filters_input.update(cx, |e, cx| e.set_text("*.*", window, cx));
        });
    });
    cx.dispatch_action(FpFindInFiles);
    cx.run_until_parked();
    let (files, total) = cx.update(|_, cx| {
        let r = ws.read(cx).results.read(cx);
        (r.sets.first().map(|s| s.files.len()).unwrap_or(0), r.sets.first().map(|s| s.total).unwrap_or(0))
    });
    assert_eq!(files, 2);
    assert_eq!(total, 3);
}

#[gpui::test]
fn select_and_find_next(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.simulate_input("foo bar foo");
    cx.simulate_keystrokes("cmd-up");
    cx.dispatch_action(SelectAndFindNext);
    let e = active(&ws, cx);
    assert_eq!(cx.update(|_, cx| e.read(cx).primary_selection().range()), 8..11);
    let _ = Selection::caret(0);
}

#[gpui::test]
fn third_party_licenses_open_read_only(cx: &mut TestAppContext) {
    let (ws, cx) = workspace(cx);
    cx.dispatch_action(ShowThirdPartyLicenses);
    let e = active(&ws, cx);
    let (text, ro, dirty) = cx.update(|_, cx| {
        let b = e.read(cx).buf(cx);
        (b.line_text(0), b.is_read_only(), b.is_dirty())
    });
    assert_eq!(text, "# Third-party licenses");
    assert!(ro && !dirty);
    // A second request re-activates the same tab.
    cx.dispatch_action(ShowThirdPartyLicenses);
    assert_eq!(cx.update(|_, cx| ws.read(cx).panes[0].items.len()), 2);
}
