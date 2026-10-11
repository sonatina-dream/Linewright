//! The "Search results" panel (Find All / Find in Files output).

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gpui::{
    div, prelude::*, px, uniform_list, Context, FocusHandle, Focusable, FontWeight, HighlightStyle, MouseButton,
    SharedString, StyledText, UniformListScrollHandle, WeakEntity, Window,
};

use super::Workspace;
use crate::search::{Hit, ResultSet};
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Set(usize),
    File(usize, usize),
    Hit(usize, usize, usize),
}

pub struct ResultsPanel {
    workspace: WeakEntity<Workspace>,
    pub sets: Vec<ResultSet>,
    rows: Vec<Row>,
    pub selected: Option<usize>,
    collapsed_files: HashSet<(usize, usize)>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    pub busy: Option<SharedString>,
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Focusable for ResultsPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ResultsPanel {
    pub fn new(workspace: WeakEntity<Workspace>, cx: &mut Context<Self>) -> Self {
        ResultsPanel {
            workspace,
            sets: Vec::new(),
            rows: Vec::new(),
            selected: None,
            collapsed_files: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            busy: None,
            cancel: None,
        }
    }

    pub fn push(&mut self, set: ResultSet, cx: &mut Context<Self>) {
        // Keep older searches, folded.
        for s in self.sets.iter_mut() {
            s.collapsed = true;
        }
        self.sets.insert(0, set);
        self.sets.truncate(20);
        self.collapsed_files.clear();
        self.rebuild();
        self.selected = None;
        self.scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
        cx.notify();
    }

    pub fn clear(&mut self, cx: &mut Context<Self>) {
        self.sets.clear();
        self.rebuild();
        self.selected = None;
        cx.notify();
    }

    fn rebuild(&mut self) {
        self.rows.clear();
        for (si, s) in self.sets.iter().enumerate() {
            self.rows.push(Row::Set(si));
            if s.collapsed {
                continue;
            }
            for (fi, f) in s.files.iter().enumerate() {
                self.rows.push(Row::File(si, fi));
                if self.collapsed_files.contains(&(si, fi)) {
                    continue;
                }
                for hi in 0..f.hits.len() {
                    self.rows.push(Row::Hit(si, fi, hi));
                }
            }
        }
    }

    fn toggle_row(&mut self, row: Row, cx: &mut Context<Self>) {
        match row {
            Row::Set(si) => {
                if let Some(s) = self.sets.get_mut(si) {
                    s.collapsed = !s.collapsed;
                }
            }
            Row::File(si, fi) => {
                if !self.collapsed_files.remove(&(si, fi)) {
                    self.collapsed_files.insert((si, fi));
                }
            }
            Row::Hit(..) => {}
        }
        self.rebuild();
        cx.notify();
    }

    fn open_row(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(ix).copied() else { return };
        self.selected = Some(ix);
        if let Row::Hit(si, fi, hi) = row {
            let f = &self.sets[si].files[fi];
            let hit = f.hits[hi].clone();
            let path = f.path.clone();
            let buffer_id = f.buffer_id;
            if let Some(ws) = self.workspace.upgrade() {
                let window_handle = window.window_handle();
                cx.defer(move |cx| {
                    let _ = window_handle.update(cx, |_, window, cx| {
                        ws.update(cx, |ws, cx| ws.open_hit(path, buffer_id, &hit, window, cx));
                    });
                });
            }
        } else {
            self.toggle_row(row, cx);
        }
        cx.notify();
    }

    /// Move the selection to the next/previous hit and open it (F4 / Shift+F4).
    pub fn step(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let n = self.rows.len();
        let mut i = self.selected.unwrap_or(if forward { n - 1 } else { 0 });
        for _ in 0..n {
            i = if forward { (i + 1) % n } else { (i + n - 1) % n };
            if matches!(self.rows[i], Row::Hit(..)) {
                self.scroll.scroll_to_item(i, gpui::ScrollStrategy::Center);
                self.open_row(i, window, cx);
                return;
            }
        }
    }

}

fn hit_text(hit: &Hit, theme: &crate::theme::AppTheme) -> StyledText {
    let prefix = format!("Line {}: ", hit.line + 1);
    let text: String = hit.text.chars().map(|c| if c == '\t' { ' ' } else { c }).collect();
    let trimmed_start = text.len() - text.trim_start().len();
    let shown = &text[trimmed_start..];
    let lead_chars = text[..trimmed_start].chars().count();
    let full = format!("{prefix}{shown}");
    let mut highlights = Vec::new();
    for r in &hit.ranges {
        let s = r.start.saturating_sub(lead_chars);
        let e = r.end.saturating_sub(lead_chars);
        let bs: usize = shown.chars().take(s).map(|c| c.len_utf8()).sum();
        let be: usize = shown.chars().take(e).map(|c| c.len_utf8()).sum();
        if be > bs {
            highlights.push((
                prefix.len() + bs..prefix.len() + be,
                HighlightStyle { color: Some(theme.match_fg), font_weight: Some(FontWeight::BOLD), background_color: Some(theme.smart_hl), ..Default::default() },
            ));
        }
    }
    StyledText::new(full).with_highlights(highlights)
}

impl Render for ResultsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        if self.rows.is_empty() && !self.sets.is_empty() {
            self.rebuild();
        }
        let rows = self.rows.clone();
        let sets: Vec<ResultSet> = self.sets.clone();
        let selected = self.selected;
        let entity = cx.entity();
        let t = theme.clone();
        let list = uniform_list("results-list", rows.len(), move |range, _window, _cx| {
            range
                .map(|ix| {
                    let row = rows[ix];
                    let entity = entity.clone();
                    let is_sel = selected == Some(ix);
                    let base = div()
                        .id(("result-row", ix))
                        .flex()
                        .items_center()
                        .h(px(20.0))
                        .w_full()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .text_size(px(12.0))
                        .when(is_sel, |d| d.bg(t.list_selected))
                        .hover(|d| d.bg(t.list_hover))
                        .on_click(move |e, window, cx| {
                            let e2 = entity.clone();
                            let dbl = e.click_count() >= 2;
                            e2.update(cx, |p, cx| {
                                if dbl || matches!(p.rows.get(ix), Some(Row::Hit(..))) {
                                    p.open_row(ix, window, cx);
                                } else {
                                    p.selected = Some(ix);
                                    if let Some(r) = p.rows.get(ix).copied() {
                                        p.toggle_row(r, cx);
                                    }
                                }
                            });
                        });
                    match row {
                        Row::Set(si) => {
                            let s = &sets[si];
                            base.pl(px(4.0))
                                .bg(t.panel_header_bg)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.panel_fg)
                                .child(ui::icon(if s.collapsed { "chevron-right" } else { "chevron-down" }, 12.0, t.muted_fg))
                                .child(div().pl(px(4.0)).child(s.title.clone()))
                        }
                        Row::File(si, fi) => {
                            let f = &sets[si].files[fi];
                            base.pl(px(18.0))
                                .text_color(t.accent)
                                .child(ui::icon("file-text", 12.0, t.muted_fg))
                                .child(div().pl(px(4.0)).child(format!("{} ({} hit{})", f.name, f.hits.len(), if f.hits.len() == 1 { "" } else { "s" })))
                        }
                        Row::Hit(si, fi, hi) => {
                            let h = &sets[si].files[fi].hits[hi];
                            base.pl(px(40.0)).text_color(t.panel_fg).font_family("Menlo").child(hit_text(h, &t))
                        }
                    }
                })
                .collect()
        })
        .track_scroll(self.scroll.clone())
        .flex_1()
        .w_full();

        let ws = self.workspace.clone();
        div()
            .id("results-panel")
            .track_focus(&self.focus_handle)
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(26.0))
                    .px(px(8.0))
                    .bg(theme.panel_header_bg)
                    .border_b_1()
                    .border_color(theme.chrome_border)
                    .child(div().text_size(px(12.0)).font_weight(FontWeight::SEMIBOLD).child("Search results"))
                    .when_some(self.busy.clone(), |d, b| d.child(ui::muted(b, &theme)))
                    .when(self.cancel.is_some(), |d| {
                        d.child(ui::button("results-stop", "Stop", &theme).h(px(20.0)).min_w(px(48.0)).on_click(cx.listener(|this, _, _, cx| {
                            if let Some(c) = &this.cancel {
                                c.store(true, Ordering::Relaxed);
                            }
                            cx.notify();
                        })))
                    })
                    .child(div().flex_1())
                    .child(
                        ui::icon_button("results-collapse", "chevron-up", &theme, false)
                            .tooltip(ui::tooltip("Fold all"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                for s in this.sets.iter_mut() {
                                    s.collapsed = true;
                                }
                                this.rebuild();
                                cx.notify();
                            })),
                    )
                    .child(
                        ui::icon_button("results-clear", "close-all", &theme, false)
                            .tooltip(ui::tooltip("Clear all"))
                            .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                    )
                    .child(ui::icon_button("results-close", "x", &theme, false).tooltip(ui::tooltip("Close")).on_click(move |_, window, cx| {
                        if let Some(ws) = ws.upgrade() {
                            ws.update(cx, |ws, cx| {
                                ws.results_visible = false;
                                ws.focus_active(window, cx);
                                cx.notify();
                            });
                        }
                    })),
            )
            .child(list)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }
}
