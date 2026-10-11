//! Headless tests that drive the editor through real key bindings and text input.

use gpui::{AppContext as _, Entity, Focusable, TestAppContext, VisualTestContext};

use super::{Editor, EditorMode};
use crate::buffer::{Buffer, Selection};
use crate::encoding::EolMode;
use crate::settings::Settings;

fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Settings::default());
        crate::theme::init(cx, crate::theme::DEFAULT_THEME);
        cx.bind_keys(crate::keymap::editor_bindings());
    });
}

fn editor_with<'a>(cx: &'a mut TestAppContext, text: &str, eol: EolMode) -> (Entity<Editor>, &'a mut VisualTestContext) {
    init(cx);
    let text = text.to_string();
    let (editor, vcx) = cx.add_window_view(move |window, cx| {
        let buffer = cx.new(|_| {
            let mut b = Buffer::new(1, "t".into(), eol, Default::default());
            b.set_text_initial(&text);
            b
        });
        Editor::new(buffer, EditorMode::Full, window, cx)
    });
    let e = editor.clone();
    vcx.update(move |window, cx| window.focus(&e.focus_handle(cx)));
    vcx.run_until_parked();
    (editor, vcx)
}

fn text(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| editor.read(cx).text(cx))
}

fn sels(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Vec<(usize, usize)> {
    cx.update(|_, cx| editor.read(cx).selections.iter().map(|s| (s.anchor, s.head)).collect())
}

fn set_sels(editor: &Entity<Editor>, cx: &mut VisualTestContext, s: Vec<Selection>) {
    let e = editor.clone();
    cx.update(move |_, cx| e.update(cx, |e, cx| e.set_selections(s, cx)));
}

#[gpui::test]
fn typing_and_undo(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "", EolMode::Lf);
    cx.simulate_input("hello");
    assert_eq!(text(&ed, cx), "hello");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("world");
    assert_eq!(text(&ed, cx), "hello\nworld");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&ed, cx), "hello\n");
    cx.simulate_keystrokes("cmd-z cmd-z");
    assert_eq!(text(&ed, cx), "");
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(text(&ed, cx), "hello");
}

#[gpui::test]
fn movement_and_selection(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "abc def\nxy\nlonger line", EolMode::Lf);
    cx.simulate_keystrokes("alt-right");
    assert_eq!(sels(&ed, cx), vec![(3, 3)]);
    cx.simulate_keystrokes("down");
    assert_eq!(sels(&ed, cx), vec![(10, 10)]); // clamped to end of "xy"
    cx.simulate_keystrokes("down");
    assert_eq!(sels(&ed, cx), vec![(14, 14)]); // goal column restored
    cx.simulate_keystrokes("cmd-left shift-cmd-right");
    assert_eq!(sels(&ed, cx), vec![(11, 22)]);
    cx.simulate_keystrokes("cmd-up");
    assert_eq!(sels(&ed, cx), vec![(0, 0)]);
}

#[gpui::test]
fn crlf_lines(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "ab\r\ncd", EolMode::Crlf);
    cx.simulate_keystrokes("cmd-right right");
    assert_eq!(sels(&ed, cx), vec![(4, 4)]); // skipped CR LF as one
    cx.simulate_keystrokes("backspace");
    assert_eq!(text(&ed, cx), "abcd");
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&ed, cx), "ab\r\ncd");
}

#[gpui::test]
fn multi_cursor_typing(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "one\ntwo\nthree", EolMode::Lf);
    cx.simulate_keystrokes("cmd-alt-down cmd-alt-down");
    assert_eq!(sels(&ed, cx).len(), 3);
    cx.simulate_input("- ");
    assert_eq!(text(&ed, cx), "- one\n- two\n- three");
    cx.simulate_keystrokes("escape");
    assert_eq!(sels(&ed, cx).len(), 1);
}

#[gpui::test]
fn column_selection_and_paste(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "aaaa\nbbbb\ncccc", EolMode::Lf);
    cx.simulate_keystrokes("right alt-shift-down alt-shift-down alt-shift-right alt-shift-right");
    assert_eq!(sels(&ed, cx), vec![(1, 3), (6, 8), (11, 13)]);
    cx.simulate_keystrokes("cmd-c");
    cx.simulate_input("X");
    assert_eq!(text(&ed, cx), "aXa\nbXb\ncXc");
    // Pasting 3 lines into 3 carets distributes them.
    cx.simulate_keystrokes("cmd-v");
    assert_eq!(text(&ed, cx), "aXaaa\nbXbbb\ncXccc");
}

#[gpui::test]
fn line_operations(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "b\na\nc\na\n", EolMode::Lf);
    cx.dispatch_action(super::SortLinesLexAsc);
    assert_eq!(text(&ed, cx), "a\na\nb\nc\n");
    cx.dispatch_action(super::RemoveDuplicateLines);
    assert_eq!(text(&ed, cx), "a\nb\nc\n");
    cx.simulate_keystrokes("cmd-d");
    assert_eq!(text(&ed, cx), "a\na\nb\nc\n");
    cx.simulate_keystrokes("down ctrl-shift-down");
    assert_eq!(text(&ed, cx), "a\nb\na\nc\n");
    cx.simulate_keystrokes("cmd-shift-l");
    assert_eq!(text(&ed, cx), "a\nb\nc\n");
}

#[gpui::test]
fn indent_comment_case(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "fn a() {\nx\n}", EolMode::Lf);
    cx.update(|_, cx| ed.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.set_syntax("Rust".into()))));
    set_sels(&ed, cx, vec![Selection::new(0, 13)]);
    cx.simulate_keystrokes("cmd-/");
    assert_eq!(text(&ed, cx), "// fn a() {\n// x\n// }");
    cx.simulate_keystrokes("cmd-/");
    assert_eq!(text(&ed, cx), "fn a() {\nx\n}");
    set_sels(&ed, cx, vec![Selection::new(9, 11)]);
    cx.simulate_keystrokes("tab");
    assert_eq!(text(&ed, cx), "fn a() {\n\tx\n}");
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(text(&ed, cx), "fn a() {\nx\n}");
    set_sels(&ed, cx, vec![Selection::new(0, 2)]);
    cx.simulate_keystrokes("cmd-shift-u");
    assert_eq!(text(&ed, cx), "FN a() {\nx\n}");
}

#[gpui::test]
fn auto_indent(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "    if x {}", EolMode::Lf);
    set_sels(&ed, cx, vec![Selection::caret(10)]);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&ed, cx), "    if x {\n    \t\n    }");
    assert_eq!(sels(&ed, cx), vec![(16, 16)]);
}

#[gpui::test]
fn folding_hides_rows(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "a:\n  b\n  c\nd", EolMode::Lf);
    cx.dispatch_action(super::FoldAll);
    cx.run_until_parked();
    let folds = cx.update(|_, cx| ed.read(cx).folds.clone());
    assert_eq!(folds.into_iter().collect::<Vec<_>>(), vec![(0, 2)]);
    cx.simulate_keystrokes("down");
    assert_eq!(sels(&ed, cx), vec![(11, 11)]); // jumped over the folded lines to "d"
}

#[gpui::test]
fn bookmarks_follow_edits(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "a\nb\nc", EolMode::Lf);
    cx.simulate_keystrokes("down cmd-f2");
    let bm = cx.update(|_, cx| ed.read(cx).buf(cx).bookmarks.iter().copied().collect::<Vec<_>>());
    assert_eq!(bm, vec![1]);
    cx.simulate_keystrokes("cmd-up enter");
    let bm = cx.update(|_, cx| ed.read(cx).buf(cx).bookmarks.iter().copied().collect::<Vec<_>>());
    assert_eq!(bm, vec![2]);
}

#[gpui::test]
fn layout_snapshot_and_click(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "hello world\nsecond", EolMode::Lf);
    cx.run_until_parked();
    let layout = cx.update(|_, cx| ed.read(cx).layout.clone()).expect("editor was laid out");
    assert!(layout.rows.len() >= 2);
    assert_eq!(layout.rows[1].line, 1);
    // Click on the second row, a few characters in.
    let tb = layout.text_bounds;
    let x = f32::from(tb.origin.x) + 4.0 + layout.space_width * 3.0 + 1.0;
    let y = f32::from(layout.rows[1].y) + layout.line_height / 2.0;
    cx.simulate_click(gpui::point(gpui::px(x), gpui::px(y)), gpui::Modifiers::default());
    assert_eq!(sels(&ed, cx), vec![(15, 15)]);
}

#[gpui::test]
fn word_wrap_rows(cx: &mut TestAppContext) {
    let long = "word ".repeat(400);
    let (ed, cx) = editor_with(cx, &long, EolMode::Lf);
    cx.update(|_, cx| crate::settings::update_settings_in_memory(cx, |s| s.word_wrap = true));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    let rows = cx.update(|_, cx| ed.read(cx).layout.as_ref().map(|l| l.row_count).unwrap_or(0));
    assert!(rows > 3, "expected wrapped rows, got {rows}");
    cx.simulate_keystrokes("down");
    let s = sels(&ed, cx)[0].1;
    assert!(s > 0 && s < long.len(), "down should move within the wrapped line, got {s}");
}

#[gpui::test]
fn auto_complete_words(cx: &mut TestAppContext) {
    let (ed, cx) = editor_with(cx, "apple application\n", EolMode::Lf);
    cx.simulate_keystrokes("cmd-down");
    cx.simulate_input("app");
    let items = cx.update(|_, cx| ed.read(cx).completion.as_ref().map(|c| c.items.clone()));
    assert_eq!(items, Some(vec!["apple".to_string(), "application".to_string()]));
    cx.simulate_keystrokes("down enter");
    assert_eq!(text(&ed, cx), "apple application\napplication");
}

#[gpui::test]
fn single_line_input_confirms(cx: &mut TestAppContext) {
    init(cx);
    let (ed, cx) = cx.add_window_view(|window, cx| Editor::single_line("Find", window, cx));
    let e = ed.clone();
    cx.update(move |window, cx| window.focus(&e.focus_handle(cx)));
    let confirmed = std::rc::Rc::new(std::cell::Cell::new(false));
    let c2 = confirmed.clone();
    let e = ed.clone();
    let _sub = cx.update(move |_, cx| {
        cx.subscribe(&e, move |_, ev: &super::EditorEvent, _| {
            if matches!(ev, super::EditorEvent::Confirm { .. }) {
                c2.set(true);
            }
        })
    });
    cx.simulate_input("abc");
    cx.simulate_keystrokes("enter");
    assert!(confirmed.get());
    assert_eq!(text(&ed, cx), "abc");
}

#[gpui::test]
fn incremental_wrap_matches_full(cx: &mut TestAppContext) {
    let mut text = String::new();
    for i in 0..60 {
        if i % 7 == 0 {
            text.push_str(&"long words here ".repeat(30));
        } else {
            text.push_str(&format!("short line {i}\tx"));
        }
        text.push('\n');
    }
    let (ed, cx) = editor_with(cx, &text, EolMode::Lf);
    cx.update(|_, cx| crate::settings::update_settings_in_memory(cx, |s| s.word_wrap = true));
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    // Edit in several places, including multi-line inserts and deletes.
    set_sels(&ed, cx, vec![Selection::caret(5), Selection::caret(900)]);
    cx.simulate_input(&"abc def ".repeat(40));
    cx.simulate_keystrokes("enter enter");
    cx.simulate_input("zz");
    cx.simulate_keystrokes("shift-up shift-up shift-up backspace");
    cx.run_until_parked();
    let incremental = cx.update(|window, cx| {
        ed.update(cx, |e, cx| {
            e.ensure_display(window, cx);
            e.display.row_count(e.buf(cx))
        })
    });
    let full = cx.update(|window, cx| {
        ed.update(cx, |e, cx| {
            e.display.invalidate();
            e.ensure_display(window, cx);
            e.display.row_count(e.buf(cx))
        })
    });
    assert_eq!(incremental, full);
    assert!(full > 61);
}
