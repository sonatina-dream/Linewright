//! Application / workspace level actions (menu commands). Editor actions live in `editor`.

use gpui::{actions, Action};

actions!(
    workspace,
    [
        // File
        NewFile,
        OpenFile,
        OpenFolder,
        ReloadFromDisk,
        Save,
        SaveAs,
        SaveCopyAs,
        SaveAll,
        RenameFile,
        CloseTab,
        CloseAll,
        CloseAllButActive,
        CloseAllButPinned,
        CloseAllToLeft,
        CloseAllToRight,
        CloseAllUnchanged,
        DeleteFromDisk,
        LoadSession,
        SaveSession,
        ReopenClosedTab,
        OpenAllRecentFiles,
        ClearRecentFiles,
        OpenContainingFolder,
        OpenInDefaultViewer,
        OpenTerminalHere,
        Print,
        Quit,
        NewWindow,
        // Edit
        CopyFullPath,
        CopyFileName,
        CopyDirPath,
        CopyAllNames,
        CopyAllPaths,
        ToggleReadOnly,
        ClearReadOnlyFlag,
        ColumnEditor,
        CharacterPanel,
        ClipboardHistoryPanel,
        OpenSelectionAsFile,
        SearchOnInternet,
        SelectAllOccurrences,
        // Search
        Find,
        Replace,
        FindInFiles,
        MarkDialog,
        FindNext,
        FindPrev,
        SelectAndFindNext,
        SelectAndFindPrev,
        FindWordNext,
        FindWordPrev,
        IncrementalSearch,
        GoToLine,
        ShowSearchResults,
        NextSearchResult,
        PrevSearchResult,
        FindCharsInRange,
        UseSelectionForFind,
        CloseFindPanel,
        MarkAllOccurrences,
        // View
        ToggleAlwaysOnTop,
        ToggleFullScreen,
        ToggleFloatingNote,
        ToggleFocusMode,
        ViewInBrowser,
        ToggleWordWrap,
        ToggleShowWhitespace,
        ToggleShowEol,
        ToggleShowAllChars,
        ToggleIndentGuides,
        ToggleWrapSymbol,
        ToggleLineNumbers,
        ToggleBookmarkMargin,
        ToggleFoldMargin,
        ToggleChangeHistory,
        ToggleCurrentLineHighlight,
        ToggleEdge,
        ToggleSmartHighlight,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        MoveToOtherView,
        CloneToOtherView,
        FocusOtherView,
        ToggleSplitOrientation,
        NextTab,
        PrevTab,
        MoveTabForward,
        MoveTabBackward,
        GoToTab1,
        GoToTab2,
        GoToTab3,
        GoToTab4,
        GoToTab5,
        GoToTab6,
        GoToTab7,
        GoToTab8,
        GoToLastTab,
        ToggleProjectFolder,
        ToggleFunctionList,
        ToggleMinimap,
        ToggleOpenDocuments,
        ToggleSearchResultsPanel,
        ToggleSyncVerticalScroll,
        ToggleSyncHorizontalScroll,
        ToggleMonitoring,
        ShowStatistics,
        ToggleToolbar,
        ToggleStatusBar,
        ToggleTabBar,
        // Encoding
        EncodeAnsi,
        EncodeUtf8,
        EncodeUtf8Bom,
        EncodeUtf16Be,
        EncodeUtf16Le,
        ConvertToAnsi,
        ConvertToUtf8,
        ConvertToUtf8Bom,
        ConvertToUtf16Be,
        ConvertToUtf16Le,
        // EOL
        EolWindows,
        EolUnix,
        EolMac,
        // Settings
        OpenPreferences,
        OpenThemes,
        EditKeymapFile,
        OpenConfigFolder,
        ToggleAutoCompletion,
        ToggleAutoCloseBrackets,
        ToggleAutoIndent,
        ToggleUseSpaces,
        // Tools
        HashMd5,
        HashSha1,
        HashSha256,
        HashSha512,
        HashMd5Files,
        HashSha256Files,
        HashMd5Selection,
        HashSha1Selection,
        HashSha256Selection,
        HashSha512Selection,
        // Macro
        StartRecording,
        StopRecording,
        ToggleRecording,
        PlaybackMacro,
        SaveMacro,
        RunMacroMultiple,
        TrimTrailingAndSave,
        // Run
        RunCommand,
        LaunchInSafari,
        LaunchInChrome,
        LaunchInFirefox,
        // Plugins
        CompareWithOtherView,
        ClearCompare,
        // Window / Help
        WindowsList,
        SortTabsByName,
        SortTabsByPath,
        About,
        ShowThirdPartyLicenses,
        ShowShortcuts,
        OpenHomepage,
        DebugInfo,
        // Misc
        HideApp,
        HideOthers,
        ShowAll,
        DismissModal,
        ShowEncodingMenu,
    ]
);

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = workspace, no_json)]
pub struct SetLanguage {
    pub name: String,
}

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = workspace, no_json)]
pub struct SetCharacterSet {
    pub name: String,
}

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = workspace, no_json)]
pub struct SetTheme {
    pub name: String,
}

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = workspace, no_json)]
pub struct OpenRecent {
    pub index: usize,
}

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = workspace, no_json)]
pub struct RunSavedMacro {
    pub index: usize,
}

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = workspace, no_json)]
pub struct ActivateTab {
    pub pane: usize,
    pub index: usize,
}

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = workspace, no_json)]
pub struct SetTabSize {
    pub size: usize,
}
