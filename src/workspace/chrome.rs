//! Rendering of the workspace chrome: toolbar, tab bars, panes, panels, status bar, popups.

use gpui::{
    anchored, deferred, div, prelude::*, px, AnyElement, Context, DragMoveEvent, ExternalPaths, FontWeight, MouseButton,
    MouseDownEvent, SharedString, Window,
};

use super::{LeftPanel, MenuEntry, RightPanel, Workspace};
use crate::actions::*;
use crate::editor;
use crate::settings::Settings;
use crate::theme::AppTheme;
use crate::ui;

#[derive(Clone)]
pub struct TabDrag {
    pub pane: usize,
    pub index: usize,
    pub name: SharedString,
}

pub struct DragPreview {
    name: SharedString,
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        div()
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(4.0))
            .bg(theme.tab_active_bg)
            .border_1()
            .border_color(theme.tab_accent)
            .text_size(px(12.0))
            .text_color(theme.tab_active_fg)
            .shadow_md()
            .child(self.name.clone())
    }
}

#[derive(Clone, Copy)]
pub enum ResizeDrag {
    Split,
    Left,
    Right,
    Bottom,
}

pub struct EmptyView;

impl Render for EmptyView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Workspace {
    pub(crate) fn render_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = crate::theme::theme(cx);
        let settings = cx.global::<Settings>().clone();
        self.update_window_title(window, cx);
        // Restore bounds (frame, not content) so the window reopens at the same size.
        let b = match window.window_bounds() {
            gpui::WindowBounds::Windowed(b) | gpui::WindowBounds::Maximized(b) | gpui::WindowBounds::Fullscreen(b) => b,
        };
        self.window_bounds = Some((f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height)));

        let mut root = div()
            .id("workspace")
            .key_context("Workspace")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.chrome_bg)
            .text_color(theme.panel_fg)
            .text_size(px(settings.ui_font_size))
            .font_family(".SystemUIFont");
        root = self.register_actions(root, window, cx);
        root = root
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                let paths = paths.paths().to_vec();
                this.open_paths(paths, window, cx);
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                if this.context_menu.take().is_some() {
                    cx.notify();
                }
            }));

        let show_chrome = !self.focus_mode;
        if settings.show_toolbar && show_chrome && !self.floating_note {
            root = root.child(self.render_toolbar(&theme, cx));
        }

        // Middle area: left panel | panes | right panel
        let mut middle = div().flex().flex_1().min_h(px(0.0)).w_full().overflow_hidden();
        let left_w = settings.left_panel_width;
        let right_w = settings.right_panel_width;
        if let (Some(lp), true) = (self.left_panel, show_chrome) {
            middle = middle.child(
                div()
                    .w(px(left_w))
                    .h_full()
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(theme.chrome_border)
                    .bg(theme.panel_bg)
                    .child(match lp {
                        LeftPanel::Folder => self.folder.clone().into_any_element(),
                        LeftPanel::DocList => self.render_doc_list(&theme, cx),
                    }),
            );
            middle = middle.child(self.resizer(ResizeDrag::Left, true, &theme));
        }
        middle = middle.child(self.render_panes(&theme, &settings, window, cx));
        if let (Some(rp), true) = (self.right_panel, show_chrome) {
            middle = middle.child(self.resizer(ResizeDrag::Right, true, &theme));
            middle = middle.child(
                div()
                    .w(px(right_w))
                    .h_full()
                    .flex_shrink_0()
                    .border_l_1()
                    .border_color(theme.chrome_border)
                    .bg(theme.panel_bg)
                    .child(match rp {
                        RightPanel::FunctionList => self.function_list.clone().into_any_element(),
                        RightPanel::DocMap => self.doc_map.clone().into_any_element(),
                        RightPanel::CharPanel => super::panels::render_char_panel(self, &theme, cx),
                        RightPanel::Clipboard => super::panels::render_clipboard_panel(self, &theme, cx),
                    }),
            );
        }
        root = root.child(
            middle
                .on_drag_move(cx.listener(|_this, e: &DragMoveEvent<ResizeDrag>, window, cx| {
                    let pos = e.event.position;
                    let b = e.bounds;
                    match e.drag(cx) {
                        ResizeDrag::Left => {
                            let w = f32::from(pos.x - b.origin.x).clamp(120.0, 700.0);
                            crate::settings::update_settings_in_memory(cx, |s| s.left_panel_width = w);
                        }
                        ResizeDrag::Right => {
                            let w = f32::from(b.origin.x + b.size.width - pos.x).clamp(120.0, 800.0);
                            crate::settings::update_settings_in_memory(cx, |s| s.right_panel_width = w);
                        }
                        _ => {}
                    }
                    let _ = window;
                    cx.notify();
                })),
        );

        if self.results_visible && show_chrome {
            root = root.child(self.resizer(ResizeDrag::Bottom, false, &theme));
            root = root.child(
                div()
                    .h(px(settings.bottom_panel_height))
                    .flex_shrink_0()
                    .w_full()
                    .border_t_1()
                    .border_color(theme.chrome_border)
                    .bg(theme.panel_bg)
                    .child(self.results.clone()),
            );
            root = root.on_drag_move(cx.listener(|_, e: &DragMoveEvent<ResizeDrag>, _, cx| {
                if let ResizeDrag::Bottom = e.drag(cx) {
                    let b = e.bounds;
                    let h = (f32::from(b.origin.y + b.size.height - e.event.position.y) - 24.0).clamp(60.0, 900.0);
                    crate::settings::update_settings_in_memory(cx, |s| s.bottom_panel_height = h);
                    cx.notify();
                }
            }));
        }
        if let Some(inc) = self.incremental.clone() {
            root = root.child(self.render_incremental_bar(&inc, &theme, cx));
        }
        if settings.show_status_bar && show_chrome && !self.floating_note {
            root = root.child(self.render_status_bar(&theme, cx));
        }

        // Overlays.
        if self.find_visible {
            root = root.child(self.render_find_overlay(cx));
        }
        if let Some((view, _, _)) = &self.modal {
            let view = view.clone();
            root = root.child(
                div()
                    .id("modal-backdrop")
                    .absolute()
                    .inset_0()
                    .bg(theme.overlay)
                    .flex()
                    .items_start()
                    .justify_center()
                    .pt(px(70.0))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(view),
            );
        }
        if let Some((msg, _)) = &self.notification {
            root = root.child(
                div().absolute().bottom(px(34.0)).right(px(16.0)).child(
                    div()
                        .max_w(px(520.0))
                        .px(px(12.0))
                        .py(px(7.0))
                        .rounded(px(6.0))
                        .bg(theme.panel_bg)
                        .border_1()
                        .border_color(theme.chrome_border)
                        .shadow_lg()
                        .text_size(px(12.5))
                        .text_color(theme.panel_fg)
                        .child(msg.clone()),
                ),
            );
        }
        if self.context_menu.is_some() {
            root = root.child(self.render_context_menu(&theme, cx));
        }
        root.into_any_element()
    }

    fn resizer(&self, kind: ResizeDrag, vertical: bool, theme: &AppTheme) -> impl IntoElement {
        let hover = theme.accent;
        let id = match kind {
            ResizeDrag::Split => "rs-split",
            ResizeDrag::Left => "rs-left",
            ResizeDrag::Right => "rs-right",
            ResizeDrag::Bottom => "rs-bottom",
        };
        let base = div().id(id).flex_shrink_0().hover(move |s| s.bg(hover));
        let base = if vertical {
            base.w(px(4.0)).h_full().cursor_col_resize()
        } else {
            base.h(px(4.0)).w_full().cursor_row_resize()
        };
        base.on_drag(kind, |_, _, _, cx| cx.new(|_| EmptyView))
    }

    fn update_window_title(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (title, edited) = match self.active_editor() {
            Some(e) => {
                let b = e.read(cx).buf(cx);
                let dirty = b.is_dirty();
                (format!("{}{} - {}", if dirty { "*" } else { "" }, b.full_name(), crate::APP_NAME), dirty)
            }
            None => (crate::APP_NAME.to_string(), false),
        };
        let dark = crate::theme::theme(cx).dark;
        if self.last_dark != Some(dark) {
            self.last_dark = Some(dark);
            crate::platform::set_window_dark(window, dark);
        }
        if title != self.last_title {
            window.set_window_title(&title);
            window.set_window_edited(edited);
            self.last_title = title;
        }
    }

    // ------------------------------------------------------------------ toolbar

    fn render_toolbar(&self, theme: &AppTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let s = cx.global::<Settings>().clone();
        let recording = cx.try_global::<editor::MacroRecorder>().map(|r| r.recording).unwrap_or(false);
        let monitoring = self.active_editor().map(|e| e.read(cx).buf(cx).monitoring).unwrap_or(false);
        let tb = |id: &'static str, icon: &'static str, tip: &'static str, active: bool, action: Box<dyn gpui::Action>| {
            ui::icon_button(id, icon, theme, active)
                .tooltip(ui::tooltip(tip))
                .on_click(move |_, window, cx| window.dispatch_action(action.boxed_clone(), cx))
                .into_any_element()
        };
        div()
            .flex()
            .items_center()
            .gap(px(1.0))
            .h(px(32.0))
            .px(px(6.0))
            .flex_shrink_0()
            .bg(theme.chrome_bg)
            .border_b_1()
            .border_color(theme.chrome_border)
            .child(tb("tb-new", "file-plus", "New (⌘N)", false, Box::new(NewFile)))
            .child(tb("tb-open", "folder-open", "Open… (⌘O)", false, Box::new(OpenFile)))
            .child(tb("tb-save", "save", "Save (⌘S)", false, Box::new(Save)))
            .child(tb("tb-saveall", "save-all", "Save All (⌘⌥S)", false, Box::new(SaveAll)))
            .child(tb("tb-close", "x", "Close (⌘W)", false, Box::new(CloseTab)))
            .child(tb("tb-closeall", "close-all", "Close All (⌘⇧W)", false, Box::new(CloseAll)))
            .child(tb("tb-print", "printer", "Print… (⌘P)", false, Box::new(Print)))
            .child(ui::vseparator(theme))
            .child(tb("tb-cut", "scissors", "Cut (⌘X)", false, Box::new(editor::Cut)))
            .child(tb("tb-copy", "copy", "Copy (⌘C)", false, Box::new(editor::Copy)))
            .child(tb("tb-paste", "clipboard", "Paste (⌘V)", false, Box::new(editor::Paste)))
            .child(ui::vseparator(theme))
            .child(tb("tb-undo", "undo", "Undo (⌘Z)", false, Box::new(editor::Undo)))
            .child(tb("tb-redo", "redo", "Redo (⌘⇧Z)", false, Box::new(editor::Redo)))
            .child(ui::vseparator(theme))
            .child(tb("tb-find", "search", "Find… (⌘F)", false, Box::new(Find)))
            .child(tb("tb-replace", "replace", "Replace… (⌘⌥F)", false, Box::new(Replace)))
            .child(ui::vseparator(theme))
            .child(tb("tb-zoomin", "zoom-in", "Zoom In (⌘+)", false, Box::new(ZoomIn)))
            .child(tb("tb-zoomout", "zoom-out", "Zoom Out (⌘-)", false, Box::new(ZoomOut)))
            .child(ui::vseparator(theme))
            .child(tb("tb-syncv", "sync-v", "Sync Vertical Scrolling", self.sync_vertical, Box::new(ToggleSyncVerticalScroll)))
            .child(tb("tb-synch", "sync-h", "Sync Horizontal Scrolling", self.sync_horizontal, Box::new(ToggleSyncHorizontalScroll)))
            .child(ui::vseparator(theme))
            .child(tb("tb-wrap", "wrap", "Word Wrap (⌥Z)", s.word_wrap, Box::new(ToggleWordWrap)))
            .child(tb("tb-allchars", "pilcrow", "Show All Invisibles", s.show_whitespace && s.show_eol, Box::new(ToggleShowAllChars)))
            .child(tb("tb-indent", "indent", "Show Indent Guides", s.show_indent_guides, Box::new(ToggleIndentGuides)))
            .child(ui::vseparator(theme))
            .child(tb("tb-docmap", "map", "Minimap", self.right_panel == Some(RightPanel::DocMap), Box::new(ToggleMinimap)))
            .child(tb("tb-funclist", "list-tree", "Function List", self.right_panel == Some(RightPanel::FunctionList), Box::new(ToggleFunctionList)))
            .child(tb("tb-folder", "panel-left", "Project Folder", self.left_panel == Some(LeftPanel::Folder), Box::new(ToggleProjectFolder)))
            .child(tb("tb-doclist", "list", "Open Documents", self.left_panel == Some(LeftPanel::DocList), Box::new(ToggleOpenDocuments)))
            .child(ui::vseparator(theme))
            .child(tb("tb-monitor", "eye", "Follow File Changes (tail -f)", monitoring, Box::new(ToggleMonitoring)))
            .child(ui::vseparator(theme))
            .child(tb("tb-rec", "record", "Start Recording (⌃⇧R)", recording, Box::new(StartRecording)))
            .child(tb("tb-stop", "stop", "Stop Recording", false, Box::new(StopRecording)))
            .child(tb("tb-play", "play", "Playback (⌃⇧P)", false, Box::new(PlaybackMacro)))
            .child(tb("tb-runmulti", "play-multi", "Run Macro Repeatedly…", false, Box::new(RunMacroMultiple)))
            .child(tb("tb-savemacro", "bookmark", "Save Recorded Macro…", false, Box::new(SaveMacro)))
    }

    // ------------------------------------------------------------------ panes & tabs

    fn render_panes(&mut self, theme: &AppTheme, settings: &Settings, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let split = self.is_split();
        let main = self.render_pane(0, theme, settings, window, cx);
        if !split {
            return div().flex_1().min_w(px(0.0)).h_full().flex().child(main).into_any_element();
        }
        let sub = self.render_pane(1, theme, settings, window, cx);
        let ratio = self.split_ratio.clamp(0.1, 0.9);
        let vertical = self.split_vertical;
        let container = div().flex_1().min_w(px(0.0)).h_full().flex();
        let container = if vertical { container.flex_row() } else { container.flex_col() };
        let first = div().flex().min_w(px(0.0)).min_h(px(0.0));
        let second = div().flex().min_w(px(0.0)).min_h(px(0.0)).flex_1();
        let (first, second) = if vertical {
            (first.h_full().w(gpui::relative(ratio)), second.h_full())
        } else {
            (first.w_full().h(gpui::relative(ratio)), second.w_full())
        };
        container
            .child(first.child(main))
            .child(self.resizer(ResizeDrag::Split, vertical, theme))
            .child(second.child(sub))
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<ResizeDrag>, _, cx| {
                if let ResizeDrag::Split = e.drag(cx) {
                    let b = e.bounds;
                    let r = if vertical {
                        f32::from(e.event.position.x - b.origin.x) / f32::from(b.size.width).max(1.0)
                    } else {
                        f32::from(e.event.position.y - b.origin.y) / f32::from(b.size.height).max(1.0)
                    };
                    this.split_ratio = r.clamp(0.1, 0.9);
                    cx.notify();
                }
            }))
            .into_any_element()
    }

    fn render_pane(&mut self, pi: usize, theme: &AppTheme, settings: &Settings, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let pane_active = self.active_pane == pi;
        let mut col = div().flex().flex_col().flex_1().min_w(px(0.0)).min_h(px(0.0)).size_full();
        if settings.show_tab_bar && !self.focus_mode {
            col = col.child(self.render_tab_bar(pi, pane_active, theme, settings, cx));
        }
        let content = match self.panes[pi].active_item() {
            Some(e) => e.clone().into_any_element(),
            None => div().into_any_element(),
        };
        col.child(div().flex_1().min_h(px(0.0)).w_full().overflow_hidden().child(content)).into_any_element()
    }

    fn render_tab_bar(&mut self, pi: usize, pane_active: bool, theme: &AppTheme, settings: &Settings, cx: &mut Context<Self>) -> impl IntoElement {
        let mut tabs: Vec<AnyElement> = Vec::new();
        let active = self.panes[pi].active;
        let count = self.panes[pi].items.len();
        for i in 0..count {
            let editor = self.panes[pi].items[i].clone();
            let (name, full, dirty, ro, monitoring) = {
                let b = editor.read(cx).buf(cx);
                (b.display_name(), b.full_name(), b.is_dirty(), b.is_read_only(), b.monitoring)
            };
            let pinned = self.pinned.contains(&editor.entity_id());
            let is_active = i == active;
            let accent = if pane_active { theme.tab_accent } else { theme.tab_accent_inactive_view };
            let (icon_name, icon_color) = if monitoring {
                ("eye", theme.accent)
            } else if ro {
                ("lock", theme.readonly)
            } else if dirty {
                ("save", theme.unsaved)
            } else {
                ("save", theme.saved)
            };
            let hover_bg = theme.tab_hover_bg;
            let drag = TabDrag { pane: pi, index: i, name: name.clone().into() };
            let drop_accent = theme.tab_accent;
            let close_hover = theme.button_hover_bg;
            let tab = div()
                .id(SharedString::from(format!("tab-{pi}-{i}")))
                .relative()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .pl(px(8.0))
                .pr(px(if settings.tab_close_button { 4.0 } else { 10.0 }))
                .h_full()
                .max_w(px(240.0))
                .border_r_1()
                .border_color(theme.chrome_border)
                .bg(if is_active { theme.tab_active_bg } else { theme.tab_bg })
                .text_color(if is_active { theme.tab_active_fg } else { theme.tab_fg })
                .when(!is_active, move |d| d.hover(move |s| s.bg(hover_bg)))
                .when(is_active, |d| d.font_weight(FontWeight::MEDIUM))
                .when(is_active, |d| d.child(div().absolute().top_0().left_0().right_0().h(px(3.0)).bg(accent)))
                .tooltip(ui::tooltip(full))
                .child(ui::icon(icon_name, 13.0, icon_color))
                .when(pinned, |d| d.child(ui::icon("pin", 11.0, theme.muted_fg)))
                .child(div().text_size(px(12.5)).whitespace_nowrap().overflow_hidden().text_ellipsis().child(name))
                .when(settings.tab_close_button, |d| {
                    d.child(
                        div()
                            .id(SharedString::from(format!("tab-close-{pi}-{i}")))
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(16.0))
                            .rounded(px(3.0))
                            .hover(move |s| s.bg(close_hover))
                            .child(ui::icon("x", 11.0, theme.muted_fg))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_tab_at(pi, i, window, cx);
                            })),
                    )
                })
                .on_click(cx.listener(move |this, e: &gpui::ClickEvent, window, cx| {
                    if e.click_count() == 2 && cx.global::<Settings>().double_click_closes_tab {
                        this.close_tab_at(pi, i, window, cx);
                    } else {
                        this.activate(pi, i, window, cx);
                    }
                }))
                .on_mouse_down(MouseButton::Middle, cx.listener(move |this, _, window, cx| {
                    this.close_tab_at(pi, i, window, cx);
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                    this.activate(pi, i, window, cx);
                    this.show_tab_context_menu(e.position, cx);
                    cx.stop_propagation();
                }))
                .on_drag(drag, |d, _, _, cx| cx.new(|_| DragPreview { name: d.name.clone() }))
                .drag_over::<TabDrag>(move |s, _, _, _| s.border_l_2().border_color(drop_accent))
                .on_drop(cx.listener(move |this, d: &TabDrag, window, cx| {
                    this.move_tab(d.pane, d.index, pi, i, window, cx);
                }));
            tabs.push(tab.into_any_element());
        }
        let drop_accent = theme.tab_accent;
        div()
            .id(SharedString::from(format!("tabbar-{pi}")))
            .flex()
            .flex_shrink_0()
            .h(px(28.0))
            .w_full()
            .bg(theme.tab_bar_bg)
            .border_b_1()
            .border_color(theme.chrome_border)
            .child(
                div()
                    .id(SharedString::from(format!("tabbar-scroll-{pi}")))
                    .flex()
                    .h_full()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_bar_scroll[pi])
                    .children(tabs)
                    .child(
                        div()
                            .id(SharedString::from(format!("tabbar-fill-{pi}")))
                            .flex_1()
                            .min_w(px(40.0))
                            .h_full()
                            .drag_over::<TabDrag>(move |s, _, _, _| s.border_l_2().border_color(drop_accent))
                            .on_drop(cx.listener(move |this, d: &TabDrag, window, cx| {
                                let end = this.panes[pi].items.len();
                                this.move_tab(d.pane, d.index, pi, end, window, cx);
                            }))
                            .on_click(cx.listener(move |this, e: &gpui::ClickEvent, window, cx| {
                                if e.click_count() == 2 {
                                    this.active_pane = pi;
                                    this.new_file(&NewFile, window, cx);
                                }
                            })),
                    ),
            )
            .child(
                ui::icon_button(SharedString::from(format!("tabbar-list-{pi}")), "chevron-down", theme, false)
                    .mx(px(2.0))
                    .my(px(2.0))
                    .tooltip(ui::tooltip("List all tabs"))
                    .on_click(cx.listener(move |this, e: &gpui::ClickEvent, _, cx| {
                        this.show_tab_list_menu(pi, e.position(), cx);
                    })),
            )
    }

    fn show_tab_list_menu(&mut self, pi: usize, pos: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let entries: Vec<MenuEntry> = self.panes[pi]
            .items
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let b = e.read(cx).buf(cx);
                MenuEntry::action(format!("{}{}", b.display_name(), if b.is_dirty() { " *" } else { "" }), ActivateTab { pane: pi, index: i })
                    .checked(i == self.panes[pi].active)
            })
            .collect();
        self.show_context_menu(pos, entries, cx);
    }

    // ------------------------------------------------------------------ status bar

    fn render_status_bar(&self, theme: &AppTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let mut cells: Vec<AnyElement> = Vec::new();
        let cell = |content: SharedString, id: &'static str| {
            div()
                .id(id)
                .px(px(8.0))
                .h_full()
                .flex()
                .items_center()
                .border_l_1()
                .border_color(theme.chrome_border)
                .whitespace_nowrap()
                .child(content)
        };
        if let Some(e) = self.active_editor() {
            let ed = e.read(cx);
            let b = ed.buf(cx);
            let sel = ed.primary_selection();
            let caret = sel.head;
            let line = b.char_to_line(caret);
            let line_start = b.line_to_char(line);
            let tab = cx.global::<Settings>().tab_size.max(1);
            let before = b.slice_string(line_start..caret);
            let col = crate::tools::tabs_to_spaces(&before, tab).chars().count() + 1;
            let lang = crate::syntax::display_name(&b.syntax_name);
            let len = b.len_chars();
            let lines = b.len_lines();
            cells.push(
                div()
                    .id("st-lang")
                    .flex_1()
                    .min_w(px(0.0))
                    .px(px(8.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .child(lang)
                    .on_click(cx.listener(|this, e: &gpui::ClickEvent, _, cx| this.show_language_menu(e.position(), cx)))
                    .into_any_element(),
            );
            cells.push(cell(format!("{} chars, {} lines", fmt_num(len), fmt_num(lines)).into(), "st-len").into_any_element());
            let sel_text = if ed.selections.len() > 1 {
                let total: usize = ed.selections.iter().map(|s| s.end() - s.start()).sum();
                format!("{} selected, {} carets", fmt_num(total), ed.selections.len())
            } else {
                let n = sel.end() - sel.start();
                let sl = if n > 0 { b.char_to_line(sel.end()) - b.char_to_line(sel.start()) + 1 } else { 0 };
                if n == 0 { String::new() } else { format!("{} selected ({} line{})", fmt_num(n), fmt_num(sl), if sl == 1 { "" } else { "s" }) }
            };
            cells.push(
                cell(format!("Ln {}, Col {}   Pos {}   {}", fmt_num(line + 1), col, fmt_num(caret + 1), sel_text).trim_end().to_string().into(), "st-pos")
                    .cursor_pointer()
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(GoToLine), cx))
                    .into_any_element(),
            );
            cells.push(
                cell(b.eol.label().into(), "st-eol")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, e: &gpui::ClickEvent, _, cx| this.show_eol_menu(e.position(), cx)))
                    .into_any_element(),
            );
            cells.push(
                cell(b.encoding.label().into(), "st-enc")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, e: &gpui::ClickEvent, _, cx| this.show_encoding_menu(e.position(), cx)))
                    .into_any_element(),
            );
            let mode = if ed.overwrite { "OVR" } else { "INS" };
            cells.push(
                cell(mode.into(), "st-ovr")
                    .w(px(44.0))
                    .justify_center()
                    .cursor_pointer()
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(editor::ToggleOverwrite), cx))
                    .into_any_element(),
            );
        }
        let recording = cx.try_global::<editor::MacroRecorder>().map(|r| r.recording).unwrap_or(false);
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .h(px(24.0))
            .w_full()
            .bg(theme.status_bg)
            .border_t_1()
            .border_color(theme.chrome_border)
            .text_size(px(12.0))
            .text_color(theme.status_fg)
            .when(recording, |d| {
                d.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .px(px(8.0))
                        .text_color(theme.unsaved)
                        .child(ui::icon("record", 10.0, theme.unsaved))
                        .child("REC"),
                )
            })
            .children(cells)
    }

    // ------------------------------------------------------------------ incremental search bar

    fn render_incremental_bar(&self, input: &gpui::Entity<editor::Editor>, theme: &AppTheme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .h(px(32.0))
            .px(px(8.0))
            .bg(theme.panel_header_bg)
            .border_t_1()
            .border_color(theme.chrome_border)
            .child(
                ui::icon_button("inc-close", "x", theme, false).on_click(cx.listener(|this, _, window, cx| {
                    this.close_incremental(window, cx);
                })),
            )
            .child(ui::label("Find:", theme))
            .child(ui::input(input, theme, cx).w(px(260.0)))
            .child(
                ui::icon_button("inc-prev", "arrow-up", theme, false)
                    .tooltip(ui::tooltip("Previous"))
                    .on_click(cx.listener(|this, _, window, cx| this.incremental_step(false, window, cx))),
            )
            .child(
                ui::icon_button("inc-next", "arrow-down", theme, false)
                    .tooltip(ui::tooltip("Next"))
                    .on_click(cx.listener(|this, _, window, cx| this.incremental_step(true, window, cx))),
            )
            .child(ui::muted(self.incremental_status.clone(), theme))
    }

    // ------------------------------------------------------------------ document list

    fn render_doc_list(&self, theme: &AppTheme, cx: &mut Context<Self>) -> AnyElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        for (pi, pane) in self.panes.iter().enumerate() {
            for (i, e) in pane.items.iter().enumerate() {
                let b = e.read(cx).buf(cx);
                let active = pi == self.active_pane && i == pane.active;
                let name = b.display_name();
                let dir = b.file.as_ref().and_then(|p| p.parent()).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                let dirty = b.is_dirty();
                rows.push(
                    div()
                        .id(SharedString::from(format!("doclist-{pi}-{i}")))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(3.0))
                        .when(active, |d| d.bg(theme.list_selected))
                        .hover(|s| s.bg(theme.list_hover))
                        .cursor_pointer()
                        .child(ui::icon("save", 12.0, if dirty { theme.unsaved } else { theme.saved }))
                        .child(div().flex_shrink_0().child(name))
                        .child(div().text_color(theme.muted_fg).text_size(px(11.0)).overflow_hidden().whitespace_nowrap().text_ellipsis().child(dir))
                        .when(pi == 1, |d| d.child(ui::muted("(sub)", theme)))
                        .on_click(cx.listener(move |this, _, window, cx| this.activate(pi, i, window, cx)))
                        .into_any_element(),
                );
            }
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(panel_header("Open Documents", theme, cx.listener(|this, _, window, cx| {
                this.left_panel = None;
                this.focus_active(window, cx);
                cx.notify();
            })))
            .child(div().id("doclist").flex_1().overflow_y_scroll().text_size(px(12.5)).children(rows))
            .into_any_element()
    }

    // ------------------------------------------------------------------ context menu

    fn render_context_menu(&self, theme: &AppTheme, cx: &mut Context<Self>) -> AnyElement {
        let Some(menu) = &self.context_menu else { return div().into_any_element() };
        let items: Vec<AnyElement> = menu
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| {
                if e.separator {
                    return div().my(px(3.0)).h(px(1.0)).bg(theme.chrome_border).into_any_element();
                }
                let enabled = e.enabled;
                let hover = theme.accent;
                let hover_fg = theme.accent_fg;
                div()
                    .id(SharedString::from(format!("ctx-{i}")))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .pl(px(6.0))
                    .pr(px(14.0))
                    .h(px(22.0))
                    .rounded(px(4.0))
                    .mx(px(4.0))
                    .text_size(px(12.5))
                    .text_color(if enabled { theme.panel_fg } else { theme.muted_fg })
                    .when(enabled, move |d| d.hover(move |s| s.bg(hover).text_color(hover_fg)).cursor_pointer())
                    .child(div().w(px(14.0)).flex().justify_center().when(e.checked, |d| d.child("✓")))
                    .child(e.label.clone())
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, window, cx| this.run_menu_entry(i, window, cx)))
                    .into_any_element()
            })
            .collect();
        deferred(
            anchored().position(menu.position).child(
                div()
                    .id("context-menu")
                    .occlude()
                    .min_w(px(menu.min_width))
                    .max_h(px(560.0))
                    .overflow_y_scroll()
                    .py(px(4.0))
                    .bg(theme.panel_bg)
                    .border_1()
                    .border_color(theme.chrome_border)
                    .rounded(px(6.0))
                    .shadow_lg()
                    .children(items)
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.context_menu = None;
                        cx.notify();
                    })),
            ),
        )
        .with_priority(10)
        .into_any_element()
    }
}

pub fn panel_header(
    title: &str,
    theme: &AppTheme,
    on_close: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_between()
        .h(px(26.0))
        .px(px(8.0))
        .bg(theme.panel_header_bg)
        .border_b_1()
        .border_color(theme.chrome_border)
        .child(div().text_size(px(12.0)).font_weight(FontWeight::SEMIBOLD).child(title.to_string()))
        .child(ui::icon_button(SharedString::from(format!("close-{title}")), "x", theme, false).on_click(on_close))
}

pub fn fmt_num(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}
