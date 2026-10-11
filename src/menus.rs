//! The native macOS menu bar.

use std::collections::BTreeMap;

use gpui::{App, Menu, MenuItem, OsAction, SystemMenuType};

use crate::actions::*;
use crate::editor::{self, MacroRecorder};
use crate::encoding::{EolMode, TextEncoding, CHARACTER_SETS};
use crate::settings::Settings;
use crate::workspace::{LeftPanel, RightPanel, Workspace};

/// Prefix marking a menu item that should get a native check mark (see `platform`).
pub const CHECK: &str = "\u{2063}";

fn item(name: &str, action: impl gpui::Action) -> MenuItem {
    MenuItem::action(name.to_string(), action)
}

fn check(name: &str, on: bool, action: impl gpui::Action) -> MenuItem {
    if on {
        MenuItem::action(format!("{CHECK}{name}"), action)
    } else {
        MenuItem::action(name.to_string(), action)
    }
}

fn sub(name: &str, items: Vec<MenuItem>) -> MenuItem {
    MenuItem::submenu(Menu { name: name.to_string().into(), items })
}

fn sep() -> MenuItem {
    MenuItem::separator()
}

struct State {
    s: Settings,
    lang: String,
    enc: Option<TextEncoding>,
    eol: Option<EolMode>,
    read_only: bool,
    monitoring: bool,
    recording: bool,
    recent: Vec<String>,
    macros: Vec<String>,
    docs: Vec<(usize, usize, String, bool)>,
    left: Option<LeftPanel>,
    right: Option<RightPanel>,
    results: bool,
    sync_v: bool,
    sync_h: bool,
    on_top: bool,
}

fn state(ws: &Workspace, cx: &App) -> State {
    let active = ws.active_editor();
    let (lang, enc, eol, read_only, monitoring) = match &active {
        Some(e) => {
            let b = e.read(cx).buf(cx);
            (b.syntax_name.clone(), Some(b.encoding), Some(b.eol), b.user_read_only, b.monitoring)
        }
        None => (String::new(), None, None, false, false),
    };
    let mut docs = Vec::new();
    for (pi, p) in ws.panes.iter().enumerate() {
        for (i, e) in p.items.iter().enumerate() {
            let b = e.read(cx).buf(cx);
            docs.push((pi, i, b.display_name(), pi == ws.active_pane && i == p.active));
        }
    }
    State {
        s: cx.global::<Settings>().clone(),
        lang,
        enc,
        eol,
        read_only,
        monitoring,
        recording: cx.try_global::<MacroRecorder>().map(|r| r.recording).unwrap_or(false),
        recent: ws.state.recent_files.clone(),
        macros: ws.state.macros.iter().map(|m| m.name.clone()).collect(),
        docs,
        left: ws.left_panel,
        right: ws.right_panel,
        results: ws.results_visible,
        sync_v: ws.sync_vertical,
        sync_h: ws.sync_horizontal,
        on_top: ws.always_on_top,
    }
}

/// A cheap fingerprint of everything the menus depend on, to avoid rebuilding needlessly.
pub fn signature(ws: &Workspace, cx: &App) -> String {
    let st = state(ws, cx);
    let s = &st.s;
    format!(
        "{}|{:?}|{:?}|{}|{}|{}|{:?}|{:?}|{:?}|{:?}|{:?}|{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}|{}|{}",
        st.lang,
        st.enc.map(|e| e.id()),
        st.eol,
        st.read_only,
        st.monitoring,
        st.recording,
        st.recent,
        st.macros,
        st.docs,
        st.left,
        st.right,
        st.results,
        st.sync_v,
        st.sync_h,
        st.on_top,
        s.word_wrap,
        s.show_whitespace,
        s.show_eol,
        s.show_indent_guides,
        s.show_wrap_symbol,
        s.show_line_numbers,
        s.show_bookmark_margin,
        s.show_fold_margin,
        s.highlight_current_line,
        s.show_edge,
        s.smart_highlight,
        s.show_toolbar,
        s.show_status_bar,
        s.show_tab_bar,
        s.show_change_history,
        s.use_spaces,
        s.auto_indent,
        s.auto_completion,
        s.tab_size,
        s.theme
    )
}

pub fn build(ws: &Workspace, cx: &App) -> Vec<Menu> {
    let st = state(ws, cx);
    let s = &st.s;

    // ---------------------------------------------------------------- app menu
    let app = Menu {
        name: crate::APP_NAME.into(),
        items: vec![
            item(&format!("About {}", crate::APP_NAME), About),
            sep(),
            item("Preferences…", OpenPreferences),
            sep(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            sep(),
            item(&format!("Hide {}", crate::APP_NAME), HideApp),
            item("Hide Others", HideOthers),
            item("Show All", ShowAll),
            sep(),
            item(&format!("Quit {}", crate::APP_NAME), Quit),
        ],
    };

    // ---------------------------------------------------------------- File
    let mut file = vec![
        item("New", NewFile),
        item("Open…", OpenFile),
        sub(
            "Show in",
            vec![item("Finder", OpenContainingFolder), item("Terminal", OpenTerminalHere)],
        ),
        item("Open in Default Viewer", OpenInDefaultViewer),
        item("Open Folder…", OpenFolder),
        item("Reload from Disk", ReloadFromDisk),
        sep(),
        item("Save", Save),
        item("Save As…", SaveAs),
        item("Save a Copy As…", SaveCopyAs),
        item("Save All", SaveAll),
        item("Rename…", RenameFile),
        item("Close", CloseTab),
        item("Close All", CloseAll),
        sub(
            "Close Others",
            vec![
                item("Close All but Current", CloseAllButActive),
                item("Close All but Pinned", CloseAllButPinned),
                item("Close All to the Left", CloseAllToLeft),
                item("Close All to the Right", CloseAllToRight),
                item("Close All Unchanged", CloseAllUnchanged),
            ],
        ),
        item("Move to Trash", DeleteFromDisk),
        sep(),
        item("Load Session…", LoadSession),
        item("Save Session…", SaveSession),
        sep(),
        item("Print…", Print),
        sep(),
    ];
    if s.max_recent_files > 0 && !st.recent.is_empty() {
        for (i, p) in st.recent.iter().enumerate().take(s.max_recent_files) {
            file.push(item(&format!("{}: {}", i + 1, p), OpenRecent { index: i }));
        }
        file.push(sep());
    }
    file.push(item("Reopen Closed Tab", ReopenClosedTab));
    file.push(item("Open All Recent Files", OpenAllRecentFiles));
    file.push(item("Clear Recent Files", ClearRecentFiles));
    file.push(sep());
    file.push(item("New Instance", NewWindow));

    // ---------------------------------------------------------------- Edit
    let edit = vec![
        MenuItem::os_action("Undo", editor::Undo, OsAction::Undo),
        MenuItem::os_action("Redo", editor::Redo, OsAction::Redo),
        sep(),
        MenuItem::os_action("Cut", editor::Cut, OsAction::Cut),
        MenuItem::os_action("Copy", editor::Copy, OsAction::Copy),
        MenuItem::os_action("Paste", editor::Paste, OsAction::Paste),
        item("Delete", editor::Delete),
        MenuItem::os_action("Select All", editor::SelectAll, OsAction::SelectAll),
        item("Set/Extend Selection Anchor", editor::ToggleSelectionAnchor),
        item("Set/Extend Column Selection Anchor", editor::ToggleColumnSelectionAnchor),
        sep(),
        sub(
            "Insert",
            vec![
                item("Date Time (short)", editor::InsertDateTimeShort),
                item("Date Time (long)", editor::InsertDateTimeLong),
                item("Date Time (customized)", editor::InsertDateTimeCustom),
            ],
        ),
        sub(
            "Copy to Clipboard",
            vec![
                item("Copy File Path", CopyFullPath),
                item("Copy File Name", CopyFileName),
                item("Copy Folder Path", CopyDirPath),
                sep(),
                item("Copy All File Names", CopyAllNames),
                item("Copy All File Paths", CopyAllPaths),
            ],
        ),
        sub("Indent", vec![item("Indent", editor::Indent), item("Outdent", editor::Outdent)]),
        sub(
            "Change Case",
            vec![
                item("UPPERCASE", editor::UpperCase),
                item("lowercase", editor::LowerCase),
                item("Title Case", editor::TitleCase),
                item("Title Case (keep capitals)", editor::TitleCaseKeepCaps),
                item("Sentence case", editor::SentenceCase),
                item("Sentence case (keep capitals)", editor::SentenceCaseKeepCaps),
                item("Invert Case", editor::InvertCase),
                item("Random Case", editor::RandomCase),
            ],
        ),
        sub(
            "Line Operations",
            vec![
                item("Duplicate Line", editor::DuplicateLine),
                item("Remove Duplicate Lines", editor::RemoveDuplicateLines),
                item("Remove Adjacent Duplicate Lines", editor::RemoveAdjacentDuplicateLines),
                item("Split Lines", editor::SplitLines),
                item("Join Lines", editor::JoinLines),
                item("Move Line Up", editor::MoveLineUp),
                item("Move Line Down", editor::MoveLineDown),
                item("Delete Line", editor::DeleteLine),
                item("Swap with Previous Line", editor::TransposeLines),
                item("Remove Empty Lines", editor::RemoveEmptyLines),
                item("Remove Blank Lines (whitespace only)", editor::RemoveBlankLines),
                item("Insert Blank Line Above", editor::InsertBlankLineAbove),
                item("Insert Blank Line Below", editor::InsertBlankLineBelow),
                item("Reverse Line Order", editor::ReverseLines),
                item("Randomize Line Order", editor::RandomizeLines),
                sep(),
                item("Sort Lines A→Z", editor::SortLinesLexAsc),
                item("Sort Lines A→Z (ignore case)", editor::SortLinesLexCiAsc),
                item("Sort Lines by Integer (ascending)", editor::SortLinesIntAsc),
                item("Sort Lines by Decimal, comma (ascending)", editor::SortLinesDecimalCommaAsc),
                item("Sort Lines by Decimal, dot (ascending)", editor::SortLinesDecimalDotAsc),
                item("Sort Lines by Length (ascending)", editor::SortLinesLengthAsc),
                sep(),
                item("Sort Lines Z→A", editor::SortLinesLexDesc),
                item("Sort Lines Z→A (ignore case)", editor::SortLinesLexCiDesc),
                item("Sort Lines by Integer (descending)", editor::SortLinesIntDesc),
                item("Sort Lines by Decimal, comma (descending)", editor::SortLinesDecimalCommaDesc),
                item("Sort Lines by Decimal, dot (descending)", editor::SortLinesDecimalDotDesc),
                item("Sort Lines by Length (descending)", editor::SortLinesLengthDesc),
            ],
        ),
        sub(
            "Comments",
            vec![
                item("Toggle Line Comment", editor::ToggleLineComment),
                item("Comment Lines", editor::LineComment),
                item("Uncomment Lines", editor::LineUncomment),
                item("Block Comment", editor::BlockComment),
                item("Block Uncomment", editor::BlockUncomment),
            ],
        ),
        sub(
            "Auto-Completion",
            vec![item("Word Completion", editor::TriggerCompletion), check("Suggest While Typing", s.auto_completion, ToggleAutoCompletion)],
        ),
        sub(
            "Line Endings",
            vec![
                check("CRLF (Windows)", st.eol == Some(EolMode::Crlf), EolWindows),
                check("LF (Unix and macOS)", st.eol == Some(EolMode::Lf), EolUnix),
                check("CR (classic Mac OS)", st.eol == Some(EolMode::Cr), EolMac),
            ],
        ),
        sub(
            "Whitespace",
            vec![
                item("Trim Trailing Whitespace", editor::TrimTrailingSpace),
                item("Trim Leading Whitespace", editor::TrimLeadingSpace),
                item("Trim Leading and Trailing Whitespace", editor::TrimLeadingAndTrailingSpace),
                item("Join Lines with Spaces", editor::JoinLinesWithSpaces),
                item("Trim and Join Lines", editor::TrimAndJoinLines),
                sep(),
                item("Convert Tabs to Spaces", editor::TabsToSpaces),
                item("Convert Spaces to Tabs", editor::SpacesToTabsAll),
                item("Convert Leading Spaces to Tabs", editor::SpacesToTabsLeading),
            ],
        ),
        sub("Selection", vec![item("Open File", OpenSelectionAsFile), item("Search on Internet", SearchOnInternet)]),
        sub(
            "Multiple Carets",
            vec![
                item("Add Next Occurrence", editor::SelectNextOccurrence),
                item("Select All Occurrences", SelectAllOccurrences),
                item("Add Caret Above", editor::AddCursorAbove),
                item("Add Caret Below", editor::AddCursorBelow),
            ],
        ),
        sep(),
        item("Insert in Column…", ColumnEditor),
        check("Character Codes", st.right == Some(RightPanel::CharPanel), CharacterPanel),
        check("Clipboard History", st.right == Some(RightPanel::Clipboard), ClipboardHistoryPanel),
        sep(),
        check("Read-Only", st.read_only, ToggleReadOnly),
        item("Make File Writable", ClearReadOnlyFlag),
        item("Toggle Overwrite Mode", editor::ToggleOverwrite),
    ];

    // ---------------------------------------------------------------- Search
    let ordinal = ["1st", "2nd", "3rd", "4th", "5th"];
    let style_all: Vec<MenuItem> = vec![
        item(&format!("Using {} Style", ordinal[0]), editor::HighlightAll1),
        item(&format!("Using {} Style", ordinal[1]), editor::HighlightAll2),
        item(&format!("Using {} Style", ordinal[2]), editor::HighlightAll3),
        item(&format!("Using {} Style", ordinal[3]), editor::HighlightAll4),
        item(&format!("Using {} Style", ordinal[4]), editor::HighlightAll5),
    ];
    let style_one: Vec<MenuItem> = vec![
        item("Color 1", editor::HighlightOne1),
        item("Color 2", editor::HighlightOne2),
        item("Color 3", editor::HighlightOne3),
        item("Color 4", editor::HighlightOne4),
        item("Color 5", editor::HighlightOne5),
    ];
    let clear_highlight: Vec<MenuItem> = vec![
        item("Clear Color 1", editor::ClearHighlight1),
        item("Clear Color 2", editor::ClearHighlight2),
        item("Clear Color 3", editor::ClearHighlight3),
        item("Clear Color 4", editor::ClearHighlight4),
        item("Clear Color 5", editor::ClearHighlight5),
        item("Clear All Highlights", editor::ClearAllHighlights),
        item("Clear Find Marks", editor::ClearFindMarks),
    ];
    let search = vec![
        item("Find…", Find),
        item("Find in Files…", FindInFiles),
        item("Find Next", FindNext),
        item("Find Previous", FindPrev),
        item("Select and Find Next", SelectAndFindNext),
        item("Select and Find Previous", SelectAndFindPrev),
        item("Find Next Occurrence of Word", FindWordNext),
        item("Find Previous Occurrence of Word", FindWordPrev),
        item("Use Selection for Find", UseSelectionForFind),
        item("Replace…", Replace),
        item("Incremental Search", IncrementalSearch),
        check("Search Results", st.results, ToggleSearchResultsPanel),
        item("Next Search Result", NextSearchResult),
        item("Previous Search Result", PrevSearchResult),
        item("Go to Line…", GoToLine),
        item("Go to Matching Brace", editor::GoToMatchingBrace),
        item("Select Inside Brackets", editor::SelectToMatchingBrace),
        item("Mark…", MarkDialog),
        sep(),
        sub("Highlight All Occurrences", style_all),
        sub("Highlight This Occurrence", style_one),
        sub("Clear Highlights", clear_highlight),
        item("Next Highlight", editor::JumpToNextMark),
        item("Previous Highlight", editor::JumpToPrevMark),
        item("Copy Highlighted Text", editor::CopyHighlightedText),
        sep(),
        sub(
            "Bookmark",
            vec![
                item("Toggle Bookmark", editor::ToggleBookmark),
                item("Next Bookmark", editor::NextBookmark),
                item("Previous Bookmark", editor::PrevBookmark),
                item("Clear All Bookmarks", editor::ClearBookmarks),
                item("Cut Bookmarked Lines", editor::CutBookmarkedLines),
                item("Copy Bookmarked Lines", editor::CopyBookmarkedLines),
                item("Replace Bookmarked Lines with Clipboard", editor::ReplaceBookmarkedLines),
                item("Remove Bookmarked Lines", editor::RemoveBookmarkedLines),
                item("Keep Only Bookmarked Lines", editor::KeepOnlyBookmarkedLines),
                item("Invert Bookmarks", editor::InvertBookmarks),
            ],
        ),
        sep(),
        item("Find Characters by Code…", FindCharsInRange),
    ];

    // ---------------------------------------------------------------- View
    let fold_levels = vec![
        item("1", editor::FoldLevel1),
        item("2", editor::FoldLevel2),
        item("3", editor::FoldLevel3),
        item("4", editor::FoldLevel4),
        item("5", editor::FoldLevel5),
        item("6", editor::FoldLevel6),
        item("7", editor::FoldLevel7),
        item("8", editor::FoldLevel8),
    ];
    let unfold_levels = vec![
        item("1", editor::UnfoldLevel1),
        item("2", editor::UnfoldLevel2),
        item("3", editor::UnfoldLevel3),
        item("4", editor::UnfoldLevel4),
        item("5", editor::UnfoldLevel5),
        item("6", editor::UnfoldLevel6),
        item("7", editor::UnfoldLevel7),
        item("8", editor::UnfoldLevel8),
    ];
    let view = vec![
        check("Always on Top", st.on_top, ToggleAlwaysOnTop),
        item("Toggle Full Screen Mode", ToggleFullScreen),
        item("Floating Note", ToggleFloatingNote),
        item("Focus Mode", ToggleFocusMode),
        sep(),
        sub(
            "Open in Browser",
            vec![
                item("Default Browser", ViewInBrowser),
                item("Safari", LaunchInSafari),
                item("Chrome", LaunchInChrome),
                item("Firefox", LaunchInFirefox),
            ],
        ),
        sep(),
        sub(
            "Show Invisibles",
            vec![
                check("Spaces and Tabs", s.show_whitespace, ToggleShowWhitespace),
                check("Line Endings", s.show_eol, ToggleShowEol),
                check("All Invisibles", s.show_whitespace && s.show_eol, ToggleShowAllChars),
                sep(),
                check("Indent Guides", s.show_indent_guides, ToggleIndentGuides),
                check("Wrap Markers", s.show_wrap_symbol, ToggleWrapSymbol),
            ],
        ),
        sub("Zoom", vec![item("Zoom In", ZoomIn), item("Zoom Out", ZoomOut), item("Restore Default Zoom", ZoomReset)]),
        sub(
            "Split View",
            vec![item("Move to Other View", MoveToOtherView), item("Clone to Other View", CloneToOtherView), item("Open in New Instance", NewWindow)],
        ),
        sub(
            "Tab",
            vec![
                item("1st Tab", GoToTab1),
                item("2nd Tab", GoToTab2),
                item("3rd Tab", GoToTab3),
                item("4th Tab", GoToTab4),
                item("5th Tab", GoToTab5),
                item("6th Tab", GoToTab6),
                item("7th Tab", GoToTab7),
                item("8th Tab", GoToTab8),
                item("Last Tab", GoToLastTab),
                sep(),
                item("Next Tab", NextTab),
                item("Previous Tab", PrevTab),
                item("Move Tab Forward", MoveTabForward),
                item("Move Tab Backward", MoveTabBackward),
            ],
        ),
        check("Word wrap", s.word_wrap, ToggleWordWrap),
        item("Focus Other View", FocusOtherView),
        item("Rotate Split (Vertical / Horizontal)", ToggleSplitOrientation),
        item("Hide Lines", editor::HideLines),
        item("Show All Hidden Lines", editor::UnhideAllLines),
        sep(),
        item("Fold All", editor::FoldAll),
        item("Unfold All", editor::UnfoldAll),
        item("Fold at Caret", editor::FoldCurrent),
        item("Unfold at Caret", editor::UnfoldCurrent),
        item("Toggle Fold at Caret", editor::ToggleFold),
        sub("Fold Level", fold_levels),
        sub("Unfold Level", unfold_levels),
        sep(),
        item("Document Statistics…", ShowStatistics),
        sep(),
        check("Project Folder", st.left == Some(LeftPanel::Folder), ToggleProjectFolder),
        check("Minimap", st.right == Some(RightPanel::DocMap), ToggleMinimap),
        check("Open Documents", st.left == Some(LeftPanel::DocList), ToggleOpenDocuments),
        check("Function List", st.right == Some(RightPanel::FunctionList), ToggleFunctionList),
        sep(),
        check("Sync Vertical Scrolling", st.sync_v, ToggleSyncVerticalScroll),
        check("Sync Horizontal Scrolling", st.sync_h, ToggleSyncHorizontalScroll),
        sep(),
        check("Follow File Changes (tail -f)", st.monitoring, ToggleMonitoring),
        sep(),
        check("Toolbar", s.show_toolbar, ToggleToolbar),
        check("Status Bar", s.show_status_bar, ToggleStatusBar),
        check("Tab Bar", s.show_tab_bar, ToggleTabBar),
        check("Line Numbers", s.show_line_numbers, ToggleLineNumbers),
        check("Bookmark Margin", s.show_bookmark_margin, ToggleBookmarkMargin),
        check("Fold Margin", s.show_fold_margin, ToggleFoldMargin),
        check("Change History", s.show_change_history, ToggleChangeHistory),
        check("Highlight Current Line", s.highlight_current_line, ToggleCurrentLineHighlight),
        check("Vertical Edge", s.show_edge, ToggleEdge),
        check("Smart Highlighting", s.smart_highlight, ToggleSmartHighlight),
    ];

    // ---------------------------------------------------------------- Encoding
    let enc = st.enc;
    let mut groups: BTreeMap<&str, Vec<MenuItem>> = BTreeMap::new();
    for (group, label, name) in CHARACTER_SETS {
        let on = enc.map(|e| e.id() == TextEncoding::from_id(name).map(|t| t.id()).unwrap_or_default()).unwrap_or(false);
        groups.entry(group).or_default().push(check(label, on, SetCharacterSet { name: name.to_string() }));
    }
    let charsets: Vec<MenuItem> = groups.into_iter().map(|(g, items)| sub(g, items)).collect();
    let encoding = vec![
        check("ANSI", enc == Some(TextEncoding::ansi()), EncodeAnsi),
        check("UTF-8", enc == Some(TextEncoding::Utf8), EncodeUtf8),
        check("UTF-8-BOM", enc == Some(TextEncoding::Utf8Bom), EncodeUtf8Bom),
        check("UTF-16 BE BOM", enc == Some(TextEncoding::Utf16BeBom), EncodeUtf16Be),
        check("UTF-16 LE BOM", enc == Some(TextEncoding::Utf16LeBom), EncodeUtf16Le),
        sub("More Encodings", charsets),
        sep(),
        item("Convert to ANSI", ConvertToAnsi),
        item("Convert to UTF-8", ConvertToUtf8),
        item("Convert to UTF-8-BOM", ConvertToUtf8Bom),
        item("Convert to UTF-16 BE BOM", ConvertToUtf16Be),
        item("Convert to UTF-16 LE BOM", ConvertToUtf16Le),
    ];

    // ---------------------------------------------------------------- Language
    let mut by_letter: BTreeMap<char, Vec<MenuItem>> = BTreeMap::new();
    for name in crate::syntax::language_names() {
        if name == crate::syntax::PLAIN_TEXT {
            continue;
        }
        let letter = name.chars().next().map(|c| c.to_ascii_uppercase()).unwrap_or('#');
        let letter = if letter.is_ascii_alphabetic() { letter } else { '#' };
        let on = name == st.lang;
        by_letter.entry(letter).or_default().push(check(&name, on, SetLanguage { name: name.clone() }));
    }
    let mut language = vec![check("Plain Text", st.lang == crate::syntax::PLAIN_TEXT, SetLanguage { name: crate::syntax::PLAIN_TEXT.into() }), sep()];
    for (letter, items) in by_letter {
        let has_current = items.iter().any(|i| matches!(i, MenuItem::Action { name, .. } if name.starts_with(CHECK)));
        let label = letter.to_string();
        language.push(if has_current { MenuItem::submenu(Menu { name: format!("{CHECK}{label}").into(), items }) } else { sub(&label, items) });
    }

    // ---------------------------------------------------------------- Settings
    let themes: Vec<MenuItem> = crate::theme::theme_names().into_iter().map(|n| check(&n, n == s.theme, SetTheme { name: n.clone() })).collect();
    let tab_sizes: Vec<MenuItem> = [2usize, 3, 4, 8].iter().map(|n| check(&n.to_string(), s.tab_size == *n, SetTabSize { size: *n })).collect();
    let settings = vec![
        item("Preferences…", OpenPreferences),
        item("Themes…", OpenThemes),
        item("Keyboard Shortcuts…", ShowShortcuts),
        sep(),
        sub("Theme", themes),
        sub("Tab Size", tab_sizes),
        check("Replace Tabs by Spaces", s.use_spaces, ToggleUseSpaces),
        check("Auto-Indent", s.auto_indent, ToggleAutoIndent),
        check("Auto-Completion", s.auto_completion, ToggleAutoCompletion),
        check("Auto-Close Brackets & Quotes", s.auto_close_brackets, ToggleAutoCloseBrackets),
        sep(),
        item("Edit keymap.json", EditKeymapFile),
        item("Open Settings Folder", OpenConfigFolder),
    ];

    // ---------------------------------------------------------------- Tools
    let tools = vec![
        sub("MD5", vec![item("Generate…", HashMd5), item("Generate from files…", HashMd5Files), item("Generate from selection into clipboard", HashMd5Selection)]),
        sub("SHA-1", vec![item("Generate…", HashSha1), item("Generate from selection into clipboard", HashSha1Selection)]),
        sub("SHA-256", vec![item("Generate…", HashSha256), item("Generate from files…", HashSha256Files), item("Generate from selection into clipboard", HashSha256Selection)]),
        sub("SHA-512", vec![item("Generate…", HashSha512), item("Generate from selection into clipboard", HashSha512Selection)]),
    ];

    // ---------------------------------------------------------------- Macro
    let mut macro_menu = vec![
        check("Start Recording", st.recording, StartRecording),
        item("Stop Recording", StopRecording),
        item("Playback", PlaybackMacro),
        item("Save Recorded Macro…", SaveMacro),
        item("Run Macro Repeatedly…", RunMacroMultiple),
        item("Trim Trailing Whitespace and Save", TrimTrailingAndSave),
    ];
    if !st.macros.is_empty() {
        macro_menu.push(sep());
        for (i, n) in st.macros.iter().enumerate() {
            macro_menu.push(item(n, RunSavedMacro { index: i }));
        }
    }

    // ---------------------------------------------------------------- Run
    let run = vec![
        item("Run…", RunCommand),
        sep(),
        item("Launch in Safari", LaunchInSafari),
        item("Launch in Chrome", LaunchInChrome),
        item("Launch in Firefox", LaunchInFirefox),
        sep(),
        item("Search on Internet", SearchOnInternet),
        item("Open Terminal Here", OpenTerminalHere),
    ];

    // ---------------------------------------------------------------- Plugins (built in)
    let plugins = vec![
        sub(
            "Encode / Decode",
            vec![
                item("Base64 Encode", editor::Base64Encode),
                item("Base64 Decode", editor::Base64Decode),
                item("URL Encode", editor::UrlEncode),
                item("URL Decode", editor::UrlDecode),
            ],
        ),
        sub("Hex", vec![item("ASCII → HEX", editor::AsciiToHex), item("HEX → ASCII", editor::HexToAscii)]),
        sub("JSON", vec![item("Format JSON", editor::JsonFormat), item("Minify JSON", editor::JsonMinify)]),
        sub("XML", vec![item("Pretty Print", editor::XmlPrettyPrint)]),
        sub("Compare", vec![item("Compare With Other View", CompareWithOtherView), item("Clear Compare Results", ClearCompare)]),
    ];

    // ---------------------------------------------------------------- Window
    let mut window = vec![item("Windows…", WindowsList), sep()];
    for (pi, i, name, active) in st.docs.iter().take(40) {
        let label = if *pi == 1 { format!("{name}  (sub view)") } else { name.clone() };
        window.push(check(&label, *active, ActivateTab { pane: *pi, index: *i }));
    }
    window.push(sep());
    window.push(item("Sort Tabs by Name", SortTabsByName));
    window.push(item("Sort Tabs by Path", SortTabsByPath));

    // ---------------------------------------------------------------- ?
    let help = vec![
        item("Keyboard Shortcuts…", ShowShortcuts),
        item(&format!("{} Home Page", crate::APP_NAME), OpenHomepage),
        item("Debug Info…", DebugInfo),
        item("Third-Party Licenses", ShowThirdPartyLicenses),
        sep(),
        item(&format!("About {}", crate::APP_NAME), About),
    ];

    let m = |name: &str, items: Vec<MenuItem>| Menu { name: name.to_string().into(), items };
    vec![
        app,
        m("File", file),
        m("Edit", edit),
        m("Search", search),
        m("View", view),
        m("Encoding", encoding),
        m("Language", language),
        m("Settings", settings),
        m("Tools", tools),
        m("Macro", macro_menu),
        m("Run", run),
        m("Extras", plugins),
        m("Window", window),
        m("Help", help),
    ]
}
