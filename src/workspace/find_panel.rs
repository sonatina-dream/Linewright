//! The Find / Replace / Find in Files / Mark dialog.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    actions, div, prelude::*, px, Context, Entity, FocusHandle, Focusable, FontWeight, MouseButton,
    Pixels, Point, SharedString, Subscription, WeakEntity, Window,
};

use super::Workspace;
use crate::editor::{Editor, EditorEvent};
use crate::search::{SearchMode, SearchOptions};
use crate::theme::AppTheme;
use crate::ui;

actions!(
    find,
    [
        FpFindNext,
        FpFindPrev,
        FpCount,
        FpFindAllCurrent,
        FpFindAllOpen,
        FpReplace,
        FpReplaceAll,
        FpReplaceAllOpen,
        FpFindInFiles,
        FpReplaceInFiles,
        FpMarkAll,
        FpClearMarks,
        FpCopyMarked,
        FpBrowseDir,
        FpUseCurrentDir,
    ]
);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FindTab {
    Find,
    Replace,
    FindInFiles,
    Mark,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatusKind {
    Info,
    Ok,
    Warn,
    Error,
}

#[derive(Clone)]
pub struct FindDrag {
    pub grab: Rc<Cell<Point<Pixels>>>,
}

pub struct FindPanel {
    workspace: WeakEntity<Workspace>,
    pub tab: FindTab,
    pub find_input: Entity<Editor>,
    pub replace_input: Entity<Editor>,
    pub filters_input: Entity<Editor>,
    pub dir_input: Entity<Editor>,
    pub opts: SearchOptions,
    pub recursive: bool,
    pub hidden: bool,
    pub bookmark_line: bool,
    pub purge: bool,
    pub status: SharedString,
    pub status_kind: StatusKind,
    focus_handle: FocusHandle,
    _subs: Vec<Subscription>,
}

impl Focusable for FindPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FindPanel {
    pub fn new(workspace: WeakEntity<Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find_input = cx.new(|cx| Editor::single_line("Find what", window, cx));
        let replace_input = cx.new(|cx| Editor::single_line("Replace with", window, cx));
        let filters_input = cx.new(|cx| Editor::single_line("*.*", window, cx));
        let dir_input = cx.new(|cx| Editor::single_line("Directory", window, cx));
        let mut subs = Vec::new();
        for (input, is_replace) in [(&find_input, false), (&replace_input, true), (&filters_input, false), (&dir_input, false)] {
            subs.push(cx.subscribe_in(input, window, move |this, _, ev: &EditorEvent, window, cx| match ev {
                EditorEvent::Confirm { shift } => {
                    let action: Box<dyn gpui::Action> = match (this.tab, is_replace, shift) {
                        (FindTab::FindInFiles, _, _) => Box::new(FpFindInFiles),
                        (FindTab::Mark, _, _) => Box::new(FpMarkAll),
                        (FindTab::Replace, true, _) => Box::new(FpReplace),
                        (_, _, true) => Box::new(FpFindPrev),
                        _ => Box::new(FpFindNext),
                    };
                    window.dispatch_action(action, cx);
                }
                EditorEvent::Cancel => window.dispatch_action(Box::new(crate::actions::CloseFindPanel), cx),
                EditorEvent::Edited | EditorEvent::Focused | EditorEvent::Blurred => cx.notify(),
                _ => {}
            }));
        }
        FindPanel {
            workspace,
            tab: FindTab::Find,
            find_input,
            replace_input,
            filters_input,
            dir_input,
            opts: SearchOptions::default(),
            recursive: true,
            hidden: false,
            bookmark_line: false,
            purge: false,
            status: SharedString::default(),
            status_kind: StatusKind::Info,
            focus_handle: cx.focus_handle(),
            _subs: subs,
        }
    }

    pub fn pattern(&self, cx: &gpui::App) -> String {
        self.find_input.read(cx).text(cx)
    }

    pub fn replacement(&self, cx: &gpui::App) -> String {
        self.replace_input.read(cx).text(cx)
    }

    pub fn set_status(&mut self, msg: impl Into<SharedString>, kind: StatusKind, cx: &mut Context<Self>) {
        self.status = msg.into();
        self.status_kind = kind;
        cx.notify();
    }

    pub fn set_pattern(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.find_input.update(cx, |e, cx| e.set_text(text, window, cx));
    }

    pub fn focus_find(&self, window: &mut Window, cx: &mut gpui::App) {
        window.focus(&self.find_input.focus_handle(cx));
        let len = self.find_input.read(cx).buf(cx).len_chars();
        self.find_input.update(cx, |e, cx| e.set_selections(vec![crate::buffer::Selection::new(0, len)], cx));
    }

    fn option_row(&self, theme: &AppTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let o = self.opts.clone();
        let tab = self.tab;
        let check = |id: &'static str, label: &'static str, on: bool, f: fn(&mut FindPanel)| {
            ui::checkbox(id, label, on, theme).on_click(cx.listener(move |this, _, _, cx| {
                f(this);
                cx.notify();
            }))
        };
        let mut col = div().flex().flex_col().gap(px(6.0));
        if tab == FindTab::Mark {
            col = col
                .child(check("fp-bookmark", "Bookmark matching lines", self.bookmark_line, |t| t.bookmark_line = !t.bookmark_line))
                .child(check("fp-purge", "Clear previous marks first", self.purge, |t| t.purge = !t.purge));
        }
        col = col
            .child(check("fp-word", "Match whole word only", o.whole_word, |t| t.opts.whole_word = !t.opts.whole_word))
            .child(check("fp-case", "Match case", o.match_case, |t| t.opts.match_case = !t.opts.match_case));
        if matches!(tab, FindTab::Find | FindTab::Replace | FindTab::Mark) {
            col = col.child(check("fp-wrap", "Wrap around", o.wrap_around, |t| t.opts.wrap_around = !t.opts.wrap_around));
        }
        if matches!(tab, FindTab::Replace | FindTab::Mark) {
            col = col.child(check("fp-insel", "In selection", o.in_selection, |t| t.opts.in_selection = !t.opts.in_selection));
        }
        if tab == FindTab::FindInFiles {
            col = col
                .child(check("fp-rec", "Include subfolders", self.recursive, |t| t.recursive = !t.recursive))
                .child(check("fp-hidden", "Include hidden folders", self.hidden, |t| t.hidden = !t.hidden));
        }
        col
    }

    fn mode_group(&self, theme: &AppTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.opts.mode;
        let radio = |id: &'static str, label: &'static str, m: SearchMode| {
            ui::radio(id, label, mode == m, theme).on_click(cx.listener(move |this, _, _, cx| {
                this.opts.mode = m;
                cx.notify();
            }))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(8.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(theme.chrome_border)
            .child(ui::section_title("SEARCH MODE", theme))
            .child(radio("fp-normal", "Normal", SearchMode::Normal))
            .child(radio("fp-ext", "Extended (\\n, \\r, \\t, \\0, \\x…)", SearchMode::Extended))
            .child(radio("fp-regex", "Regular expression", SearchMode::Regex))
            .child(
                div().pl(px(20.0)).child(
                    ui::checkbox("fp-dot", ". matches newline", self.opts.dot_matches_newline, theme).on_click(cx.listener(|this, _, _, cx| {
                        this.opts.dot_matches_newline = !this.opts.dot_matches_newline;
                        cx.notify();
                    })),
                ),
            )
    }

    fn direction_group(&self, theme: &AppTheme, cx: &mut Context<Self>) -> impl IntoElement {
        let back = self.opts.backward;
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(8.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(theme.chrome_border)
            .child(ui::section_title("DIRECTION", theme))
            .child(ui::radio("fp-up", "Backward", back, theme).on_click(cx.listener(|this, _, _, cx| {
                this.opts.backward = true;
                cx.notify();
            })))
            .child(ui::radio("fp-down", "Forward", !back, theme).on_click(cx.listener(|this, _, _, cx| {
                this.opts.backward = false;
                cx.notify();
            })))
    }

    fn visible_inputs(&self) -> Vec<Entity<Editor>> {
        let mut v = vec![self.find_input.clone()];
        if matches!(self.tab, FindTab::Replace | FindTab::FindInFiles) {
            v.push(self.replace_input.clone());
        }
        if self.tab == FindTab::FindInFiles {
            v.push(self.filters_input.clone());
            v.push(self.dir_input.clone());
        }
        v
    }

    fn cycle_focus(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let inputs = self.visible_inputs();
        let current = inputs.iter().position(|e| e.focus_handle(cx).is_focused(window)).unwrap_or(0);
        let n = inputs.len();
        let next = if forward { (current + 1) % n } else { (current + n - 1) % n };
        let target = inputs[next].clone();
        window.focus(&target.focus_handle(cx));
        let len = target.read(cx).buf(cx).len_chars();
        target.update(cx, |e, cx| e.set_selections(vec![crate::buffer::Selection::new(0, len)], cx));
    }

    fn labeled_input(label: &'static str, input: &Entity<Editor>, theme: &AppTheme, cx: &gpui::App) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(div().w(px(92.0)).flex_shrink_0().text_size(px(12.5)).child(label))
            .child(ui::input(input, theme, cx).flex_1())
    }
}

fn act(id: &'static str, label: &'static str, action: impl gpui::Action + Clone, theme: &AppTheme, primary: bool) -> gpui::Stateful<gpui::Div> {
    let b = if primary { ui::primary_button(id, label, theme) } else { ui::button(id, label, theme) };
    b.w_full().on_click(move |_, window, cx| window.dispatch_action(Box::new(action.clone()), cx))
}

impl Render for FindPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let tab = self.tab;
        let tab_button = |id: &'static str, label: &'static str, t: FindTab| {
            let active = tab == t;
            div()
                .id(id)
                .px(px(12.0))
                .py(px(5.0))
                .text_size(px(12.5))
                .cursor_pointer()
                .rounded_t(px(5.0))
                .when(active, |d| d.bg(theme.dialog_bg).border_1().border_b_0().border_color(theme.chrome_border).font_weight(FontWeight::MEDIUM))
                .when(!active, |d| d.text_color(theme.muted_fg).hover(|s| s.text_color(theme.panel_fg)))
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.tab = t;
                    if t == FindTab::FindInFiles && this.dir_input.read(cx).buf(cx).len_chars() == 0 {
                        window.dispatch_action(Box::new(FpUseCurrentDir), cx);
                    }
                    cx.notify();
                }))
        };
        let drag = FindDrag { grab: Rc::new(Cell::new(Point::default())) };
        let title_bar = div()
            .id("fp-title")
            .flex()
            .items_end()
            .justify_between()
            .pl(px(8.0))
            .pr(px(4.0))
            .pt(px(6.0))
            .bg(theme.panel_header_bg)
            .border_b_1()
            .border_color(theme.chrome_border)
            .cursor_move()
            .on_drag(drag, |d, offset, _, cx| {
                d.grab.set(offset);
                cx.new(|_| super::chrome::EmptyView)
            })
            .child(
                div()
                    .flex()
                    .child(tab_button("fp-tab-find", "Find", FindTab::Find))
                    .child(tab_button("fp-tab-replace", "Replace", FindTab::Replace))
                    .child(tab_button("fp-tab-fif", "Find in Files", FindTab::FindInFiles))
                    .child(tab_button("fp-tab-mark", "Mark", FindTab::Mark)),
            )
            .child(
                div().pb(px(3.0)).child(
                    ui::icon_button("fp-close", "x", &theme, false)
                        .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::actions::CloseFindPanel), cx)),
                ),
            );

        let mut inputs = div().flex().flex_col().gap(px(8.0)).flex_1().min_w(px(0.0));
        inputs = inputs.child(Self::labeled_input("Find what:", &self.find_input, &theme, cx));
        if matches!(tab, FindTab::Replace | FindTab::FindInFiles) {
            inputs = inputs.child(Self::labeled_input("Replace with:", &self.replace_input, &theme, cx));
        }
        if tab == FindTab::FindInFiles {
            inputs = inputs.child(Self::labeled_input("Filters:", &self.filters_input, &theme, cx));
            inputs = inputs.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().flex_1().min_w(px(0.0)).child(Self::labeled_input("Directory:", &self.dir_input, &theme, cx)))
                    .child(ui::button("fp-browse", "…", &theme).min_w(px(28.0)).on_click(|_, window, cx| window.dispatch_action(Box::new(FpBrowseDir), cx)))
                    .child(
                        ui::button("fp-curdir", "Current", &theme)
                            .min_w(px(56.0))
                            .tooltip(ui::tooltip("Follow current document"))
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(FpUseCurrentDir), cx)),
                    ),
            );
        }
        let lower = div()
            .flex()
            .gap(px(12.0))
            .child(self.option_row(&theme, cx))
            .child(div().flex().flex_col().gap(px(8.0)).child(self.mode_group(&theme, cx)).when(tab == FindTab::Find, |d| d.child(self.direction_group(&theme, cx))));
        inputs = inputs.child(lower);

        let buttons = match tab {
            FindTab::Find => div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(act("fp-b-next", "Find Next", FpFindNext, &theme, true))
                .child(act("fp-b-prev", "Find Previous", FpFindPrev, &theme, false))
                .child(act("fp-b-count", "Count", FpCount, &theme, false))
                .child(act("fp-b-allopen", "Find All in Open Documents", FpFindAllOpen, &theme, false))
                .child(act("fp-b-allcur", "Find All in Current Document", FpFindAllCurrent, &theme, false)),
            FindTab::Replace => div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(act("fp-b-next", "Find Next", FpFindNext, &theme, true))
                .child(act("fp-b-replace", "Replace", FpReplace, &theme, false))
                .child(act("fp-b-replall", "Replace All", FpReplaceAll, &theme, false))
                .child(act("fp-b-replopen", "Replace All in Open Documents", FpReplaceAllOpen, &theme, false)),
            FindTab::FindInFiles => div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(act("fp-b-fif", "Find All", FpFindInFiles, &theme, true))
                .child(act("fp-b-rif", "Replace in Files", FpReplaceInFiles, &theme, false)),
            FindTab::Mark => div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(act("fp-b-mark", "Mark All", FpMarkAll, &theme, true))
                .child(act("fp-b-clear", "Clear Marks", FpClearMarks, &theme, false))
                .child(act("fp-b-copy", "Copy Marked Text", FpCopyMarked, &theme, false)),
        };
        let status_color = match self.status_kind {
            StatusKind::Info => theme.muted_fg,
            StatusKind::Ok => crate::theme::rgb_hex(0x2E9D4F),
            StatusKind::Warn => crate::theme::rgb_hex(0xC07A00),
            StatusKind::Error => theme.unsaved,
        };
        let _ = &self.workspace;
        div()
            .id("find-panel")
            .key_context("FindPanel")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &crate::editor::Tab, window, cx| this.cycle_focus(true, window, cx)))
            .on_action(cx.listener(|this, _: &crate::editor::ShiftTab, window, cx| this.cycle_focus(false, window, cx)))
            .occlude()
            .w(px(700.0))
            .flex()
            .flex_col()
            .bg(theme.dialog_bg)
            .rounded(px(8.0))
            .border_1()
            .border_color(theme.chrome_border)
            .shadow_lg()
            .overflow_hidden()
            .text_color(theme.panel_fg)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(title_bar)
            .child(div().flex().gap(px(12.0)).p(px(12.0)).child(inputs).child(buttons.w(px(236.0)).flex_shrink_0()))
            .child(
                div()
                    .h(px(24.0))
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .border_t_1()
                    .border_color(theme.chrome_border)
                    .bg(theme.panel_header_bg)
                    .text_size(px(12.0))
                    .text_color(status_color)
                    .child(self.status.clone()),
            )
    }
}
