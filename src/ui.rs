//! Small reusable UI building blocks (buttons, check boxes, radio buttons, inputs, tooltips).

use gpui::{
    div, prelude::*, px, svg, AnyView, App, Div, ElementId, Entity, FontWeight, Hsla, SharedString, Stateful, Window,
};

use crate::assets::icon_path;
use crate::editor::Editor;
use crate::theme::AppTheme;

pub fn icon(name: &str, size: f32, color: Hsla) -> impl IntoElement {
    svg().path(icon_path(name)).size(px(size)).flex_shrink_0().text_color(color)
}

pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &AppTheme) -> Stateful<Div> {
    let t = theme.clone();
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .px(px(12.0))
        .h(px(24.0))
        .min_w(px(72.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(theme.button_border)
        .bg(theme.button_bg)
        .text_color(theme.panel_fg)
        .text_size(px(12.5))
        .cursor_pointer()
        .hover(move |s| s.bg(t.button_hover_bg))
        .child(label.into())
}

pub fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>, theme: &AppTheme) -> Stateful<Div> {
    let mut hover = theme.accent;
    hover.l = (hover.l + 0.06).min(1.0);
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .px(px(12.0))
        .h(px(24.0))
        .min_w(px(72.0))
        .rounded(px(4.0))
        .bg(theme.accent)
        .text_color(theme.accent_fg)
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .child(label.into())
}

pub fn icon_button(id: impl Into<ElementId>, icon_name: &str, theme: &AppTheme, active: bool) -> Stateful<Div> {
    let t = theme.clone();
    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .size(px(24.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .when(active, |d| d.bg(theme.list_selected).border_1().border_color(theme.accent))
        .hover(move |s| s.bg(t.button_hover_bg))
        .child(icon(icon_name, 15.0, if active { theme.accent } else { theme.panel_fg }))
}

pub fn checkbox(id: impl Into<ElementId>, label: impl Into<SharedString>, checked: bool, theme: &AppTheme) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .text_size(px(12.5))
        .text_color(theme.panel_fg)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(14.0))
                .rounded(px(3.0))
                .border_1()
                .border_color(if checked { theme.accent } else { theme.input_border })
                .bg(if checked { theme.accent } else { theme.input_bg })
                .when(checked, |d| d.child(icon("check", 11.0, theme.accent_fg))),
        )
        .child(label.into())
}

pub fn radio(id: impl Into<ElementId>, label: impl Into<SharedString>, checked: bool, theme: &AppTheme) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .text_size(px(12.5))
        .text_color(theme.panel_fg)
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(14.0))
                .rounded(px(7.0))
                .border_1()
                .border_color(if checked { theme.accent } else { theme.input_border })
                .bg(theme.input_bg)
                .when(checked, |d| d.child(div().size(px(6.0)).rounded(px(3.0)).bg(theme.accent))),
        )
        .child(label.into())
}

/// A bordered frame around a single-line [`Editor`].
pub fn input(editor: &Entity<Editor>, theme: &AppTheme, cx: &App) -> Div {
    let focused = editor.read(cx).focused;
    div()
        .flex()
        .items_center()
        .h(px(24.0))
        .px(px(6.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if focused { theme.focus_border } else { theme.input_border })
        .bg(theme.input_bg)
        .overflow_hidden()
        .child(div().flex_1().overflow_hidden().child(editor.clone()))
}

/// A bordered frame around a multi-line [`Editor`].
pub fn text_area(editor: &Entity<Editor>, height: f32, theme: &AppTheme, cx: &App) -> Div {
    let focused = editor.read(cx).focused;
    div()
        .h(px(height))
        .p(px(4.0))
        .rounded(px(4.0))
        .border_1()
        .border_color(if focused { theme.focus_border } else { theme.input_border })
        .bg(theme.input_bg)
        .overflow_hidden()
        .child(editor.clone())
}

pub fn label(text: impl Into<SharedString>, theme: &AppTheme) -> Div {
    div().text_size(px(12.5)).text_color(theme.panel_fg).child(text.into())
}

pub fn muted(text: impl Into<SharedString>, theme: &AppTheme) -> Div {
    div().text_size(px(11.5)).text_color(theme.muted_fg).child(text.into())
}

pub fn section_title(text: impl Into<SharedString>, theme: &AppTheme) -> Div {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.muted_fg)
        .child(text.into())
}

pub fn vseparator(theme: &AppTheme) -> Div {
    div().w(px(1.0)).h(px(18.0)).mx(px(3.0)).bg(theme.chrome_border)
}

pub struct Tooltip {
    text: SharedString,
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        div()
            .px(px(8.0))
            .py(px(4.0))
            .rounded(px(4.0))
            .bg(theme.panel_bg)
            .border_1()
            .border_color(theme.chrome_border)
            .shadow_md()
            .text_size(px(12.0))
            .text_color(theme.panel_fg)
            .child(self.text.clone())
    }
}

pub fn tooltip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |_window, cx| cx.new(|_| Tooltip { text: text.clone() }).into()
}

/// A dialog frame with title bar, content and button row.
pub fn dialog_frame(title: impl Into<SharedString>, width: f32, theme: &AppTheme) -> Div {
    div()
        .w(px(width))
        .flex()
        .flex_col()
        .bg(theme.dialog_bg)
        .rounded(px(8.0))
        .border_1()
        .border_color(theme.chrome_border)
        .shadow_lg()
        .overflow_hidden()
        .child(
            div()
                .flex()
                .items_center()
                .h(px(32.0))
                .px(px(12.0))
                .bg(theme.panel_header_bg)
                .border_b_1()
                .border_color(theme.chrome_border)
                .text_size(px(13.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.panel_fg)
                .child(title.into()),
        )
}
