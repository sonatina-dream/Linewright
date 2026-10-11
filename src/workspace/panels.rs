//! Docking panels: Project Folder, Function List, Minimap, Character Codes,
//! Clipboard History.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    canvas, div, fill, point, prelude::*, px, size, AnyElement, Bounds, Context, Entity, FocusHandle,
    Focusable, MouseButton, MouseDownEvent, MouseMoveEvent, SharedString, UniformListScrollHandle, WeakEntity, Window,
    uniform_list,
};

use super::chrome::panel_header;
use super::{MenuEntry, Workspace};
use crate::editor::{Editor, EditorEvent};
use crate::theme::AppTheme;
use crate::ui;

// ====================================================================== Project Folder

#[derive(Clone)]
struct FolderEntry {
    path: PathBuf,
    depth: usize,
    is_dir: bool,
    name: SharedString,
    is_root: bool,
}

pub struct FolderPanel {
    workspace: WeakEntity<Workspace>,
    pub roots: Vec<PathBuf>,
    expanded: HashSet<PathBuf>,
    cache: HashMap<PathBuf, Vec<(PathBuf, bool)>>,
    entries: Vec<FolderEntry>,
    selected: Option<PathBuf>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    show_hidden: bool,
}

impl Focusable for FolderPanel {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl FolderPanel {
    pub fn new(workspace: WeakEntity<Workspace>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        FolderPanel {
            workspace,
            roots: Vec::new(),
            expanded: HashSet::new(),
            cache: HashMap::new(),
            entries: Vec::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            show_hidden: false,
        }
    }

    pub fn add_root(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if !self.roots.contains(&path) {
            self.expanded.insert(path.clone());
            self.roots.push(path);
        }
        self.rebuild();
        cx.notify();
    }

    pub fn remove_root(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.roots.retain(|r| r != path);
        self.rebuild();
        cx.notify();
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.cache.clear();
        self.rebuild();
        cx.notify();
    }

    fn children(&mut self, dir: &Path) -> Vec<(PathBuf, bool)> {
        if let Some(c) = self.cache.get(dir) {
            return c.clone();
        }
        let mut out: Vec<(PathBuf, bool)> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| self.show_hidden || !e.file_name().to_string_lossy().starts_with('.'))
                    .map(|e| {
                        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        (e.path(), is_dir)
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.file_name().map(|n| n.to_string_lossy().to_lowercase()).cmp(&b.0.file_name().map(|n| n.to_string_lossy().to_lowercase()))));
        self.cache.insert(dir.to_path_buf(), out.clone());
        out
    }

    fn rebuild(&mut self) {
        let mut entries = Vec::new();
        let roots = self.roots.clone();
        for root in roots {
            let name: SharedString = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| root.to_string_lossy().to_string()).into();
            entries.push(FolderEntry { path: root.clone(), depth: 0, is_dir: true, name, is_root: true });
            if self.expanded.contains(&root) {
                self.push_children(&root, 1, &mut entries);
            }
        }
        self.entries = entries;
    }

    fn push_children(&mut self, dir: &Path, depth: usize, out: &mut Vec<FolderEntry>) {
        if depth > 40 || out.len() > 200_000 {
            return;
        }
        for (p, is_dir) in self.children(dir) {
            let name: SharedString = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default().into();
            out.push(FolderEntry { path: p.clone(), depth, is_dir, name, is_root: false });
            if is_dir && self.expanded.contains(&p) {
                self.push_children(&p, depth + 1, out);
            }
        }
    }

    fn click(&mut self, ix: usize, double: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.entries.get(ix).cloned() else { return };
        self.selected = Some(e.path.clone());
        if e.is_dir {
            if !self.expanded.remove(&e.path) {
                self.expanded.insert(e.path.clone());
            }
            self.rebuild();
        } else if double || true {
            let ws = self.workspace.clone();
            let path = e.path.clone();
            let handle = window.window_handle();
            cx.defer(move |cx| {
                let _ = handle.update(cx, |_, window, cx| {
                    if let Some(ws) = ws.upgrade() {
                        ws.update(cx, |ws, cx| ws.open_paths(vec![path], window, cx));
                    }
                });
            });
        }
        cx.notify();
    }

    fn context_menu(&mut self, ix: usize, pos: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let Some(e) = self.entries.get(ix).cloned() else { return };
        self.selected = Some(e.path.clone());
        let p = e.path.clone();
        let dir = if e.is_dir { e.path.clone() } else { e.path.parent().map(|d| d.to_path_buf()).unwrap_or_default() };
        let mut entries = Vec::new();
        if !e.is_dir {
            let p2 = p.clone();
            entries.push(MenuEntry::callback("Open", move |ws, w, cx| ws.open_paths(vec![p2.clone()], w, cx)));
        }
        let p2 = p.clone();
        entries.push(MenuEntry::callback("Reveal in Finder", move |_, _, cx| cx.reveal_path(&p2)));
        let p2 = p.clone();
        entries.push(MenuEntry::callback("Copy Path", move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(p2.to_string_lossy().to_string()))));
        let d2 = dir.clone();
        entries.push(MenuEntry::callback("Find in Files…", move |ws, w, cx| {
            ws.find_in_files(&crate::actions::FindInFiles, w, cx);
            let s = d2.to_string_lossy().to_string();
            let input = ws.find_panel.read(cx).dir_input.clone();
            input.update(cx, |e, cx| e.set_text(&s, w, cx));
        }));
        let d2 = dir.clone();
        entries.push(MenuEntry::callback("Open in Terminal", move |_, _, _| {
            let _ = std::process::Command::new("open").arg("-a").arg("Terminal").arg(&d2).spawn();
        }));
        entries.push(MenuEntry::separator());
        let d2 = dir.clone();
        entries.push(MenuEntry::callback("New File…", move |ws, w, cx| {
            let view = cx.new(|cx| super::dialogs::InputDialog::new(ws.weak_self.clone(), "New File", "File name:", "", super::dialogs::InputPurpose::NewFileIn(d2.clone()), w, cx));
            ws.open_modal(view, w, cx);
        }));
        let d2 = dir.clone();
        entries.push(MenuEntry::callback("New Folder…", move |ws, w, cx| {
            let view = cx.new(|cx| super::dialogs::InputDialog::new(ws.weak_self.clone(), "New Folder", "Folder name:", "", super::dialogs::InputPurpose::NewFolderIn(d2.clone()), w, cx));
            ws.open_modal(view, w, cx);
        }));
        if !e.is_root {
            let p2 = p.clone();
            let name = e.name.to_string();
            entries.push(MenuEntry::callback("Rename…", move |ws, w, cx| {
                let view = cx.new(|cx| super::dialogs::InputDialog::new(ws.weak_self.clone(), "Rename", "New name:", &name, super::dialogs::InputPurpose::RenamePath(p2.clone()), w, cx));
                ws.open_modal(view, w, cx);
            }));
            let p2 = p.clone();
            entries.push(MenuEntry::callback("Move to Trash", move |ws, _, cx| {
                match trash::delete(&p2) {
                    Ok(()) => ws.folder.update(cx, |f, cx| f.refresh(cx)),
                    Err(err) => ws.notify_message(format!("Delete failed: {err}"), cx),
                }
            }));
        }
        entries.push(MenuEntry::separator());
        if e.is_root {
            let p2 = p.clone();
            entries.push(MenuEntry::callback("Remove from Workspace", move |ws, _, cx| ws.folder.update(cx, |f, cx| f.remove_root(&p2, cx))));
        }
        entries.push(MenuEntry::callback("Refresh", |ws, _, cx| ws.folder.update(cx, |f, cx| f.refresh(cx))));
        let ws = self.workspace.clone();
        cx.defer(move |cx| {
            if let Some(ws) = ws.upgrade() {
                ws.update(cx, |ws, cx| ws.show_context_menu(pos, entries, cx));
            }
        });
    }
}

impl Render for FolderPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let entries = self.entries.clone();
        let selected = self.selected.clone();
        let entity = cx.entity();
        let t = theme.clone();
        let list = uniform_list("folder-tree", entries.len(), move |range, _, _| {
            range
                .map(|ix| {
                    let e = &entries[ix];
                    let entity = entity.clone();
                    let entity2 = entity.clone();
                    let is_sel = selected.as_ref() == Some(&e.path);
                    div()
                        .id(("folder-row", ix))
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .h(px(22.0))
                        .pl(px(6.0 + e.depth as f32 * 14.0))
                        .pr(px(6.0))
                        .whitespace_nowrap()
                        .text_size(px(12.5))
                        .cursor_pointer()
                        .when(is_sel, |d| d.bg(t.list_selected))
                        .hover(|d| d.bg(t.list_hover))
                        .when(e.is_root, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                        .child(if e.is_dir {
                            ui::icon("chevron-right", 11.0, t.muted_fg).into_any_element()
                        } else {
                            div().w(px(11.0)).into_any_element()
                        })
                        .child(ui::icon(if e.is_dir { "folder" } else { "file" }, 13.0, if e.is_dir { crate::theme::rgb_hex(0xD8A630) } else { t.muted_fg }))
                        .child(e.name.clone())
                        .on_click(move |ev, window, cx| {
                            let dbl = ev.click_count() >= 2;
                            entity.update(cx, |p, cx| p.click(ix, dbl, window, cx));
                        })
                        .on_mouse_down(MouseButton::Right, move |ev: &MouseDownEvent, _, cx| {
                            let pos = ev.position;
                            entity2.update(cx, |p, cx| p.context_menu(ix, pos, cx));
                            cx.stop_propagation();
                        })
                })
                .collect()
        })
        .track_scroll(self.scroll.clone())
        .flex_1();
        let ws = self.workspace.clone();
        let ws2 = self.workspace.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .track_focus(&self.focus_handle)
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .h(px(26.0))
                    .px(px(8.0))
                    .gap(px(2.0))
                    .bg(theme.panel_header_bg)
                    .border_b_1()
                    .border_color(theme.chrome_border)
                    .child(div().flex_1().text_size(px(12.0)).font_weight(gpui::FontWeight::SEMIBOLD).child("Project Folder"))
                    .child(ui::icon_button("fw-add", "plus", &theme, false).tooltip(ui::tooltip("Add folder…")).on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::OpenFolder), cx)
                    }))
                    .child(ui::icon_button("fw-refresh", "refresh", &theme, false).tooltip(ui::tooltip("Refresh")).on_click(cx.listener(|this, _, _, cx| this.refresh(cx))))
                    .child(ui::icon_button("fw-collapse", "chevron-up", &theme, false).tooltip(ui::tooltip("Collapse all")).on_click(cx.listener(|this, _, _, cx| {
                        this.expanded.clear();
                        this.rebuild();
                        cx.notify();
                    })))
                    .child(ui::icon_button("fw-hidden", "eye", &theme, self.show_hidden).tooltip(ui::tooltip("Show hidden files")).on_click(cx.listener(|this, _, _, cx| {
                        this.show_hidden = !this.show_hidden;
                        this.refresh(cx);
                    })))
                    .child(ui::icon_button("fw-close", "x", &theme, false).on_click(move |_, window, cx| {
                        if let Some(ws) = ws.upgrade() {
                            ws.update(cx, |ws, cx| {
                                ws.left_panel = None;
                                ws.focus_active(window, cx);
                                cx.notify();
                            });
                        }
                    })),
            )
            .when(self.roots.is_empty(), |d| {
                d.child(
                    div().p(px(12.0)).flex().flex_col().gap(px(8.0)).child(ui::muted("No folder opened.", &theme)).child(
                        ui::button("fw-open", "Open Folder…", &theme).on_click(move |_, window, cx| {
                            let _ = &ws2;
                            window.dispatch_action(Box::new(crate::actions::OpenFolder), cx)
                        }),
                    ),
                )
            })
            .child(list)
    }
}

// ====================================================================== Function List

#[derive(Clone)]
pub struct FunctionItem {
    pub name: SharedString,
    pub line: usize,
}

pub struct FunctionListPanel {
    workspace: WeakEntity<Workspace>,
    items: Vec<FunctionItem>,
    filter: Option<Entity<Editor>>,
    sort_alpha: bool,
    generation: u64,
    scroll: UniformListScrollHandle,
    language: String,
}

impl FunctionListPanel {
    pub fn new(workspace: WeakEntity<Workspace>, _cx: &mut Context<Self>) -> Self {
        FunctionListPanel { workspace, items: Vec::new(), filter: None, sort_alpha: false, generation: 0, scroll: UniformListScrollHandle::new(), language: String::new() }
    }

    pub fn schedule_refresh(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let Ok(Some((text, lang))) = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return None;
                }
                let ws = this.workspace.upgrade()?;
                let visible = ws.read(cx).right_panel == Some(super::RightPanel::FunctionList);
                if !visible {
                    return None;
                }
                let e = ws.read(cx).active_editor()?;
                let lang = e.read(cx).buf(cx).syntax_name.clone();
                let text = e.update(cx, |e, cx| e.buffer.update(cx, |b, _| b.text_arc()));
                Some((text, lang))
            }) else {
                return;
            };
            let lang2 = lang.clone();
            let items = cx.background_executor().spawn(async move { extract_functions(&text, &lang2) }).await;
            let _ = this.update(cx, |this, cx| {
                if this.generation == generation {
                    this.items = items;
                    this.language = lang;
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

const NOT_FUNCTIONS: &[&str] = &["if", "for", "while", "switch", "return", "else", "catch", "sizeof", "do", "new", "delete", "case", "using", "typedef", "throw", "elif", "foreach", "lock", "synchronized", "defined"];

pub fn extract_functions(text: &str, lang: &str) -> Vec<FunctionItem> {
    let Some(pat) = crate::syntax::function_list_pattern(lang) else { return Vec::new() };
    let Ok(re) = regex::Regex::new(pat) else { return Vec::new() };
    let mut out = Vec::new();
    let mut line = 0usize;
    let mut last = 0usize;
    for caps in re.captures_iter(text) {
        let Some(m) = caps.get(0) else { continue };
        let name = re
            .capture_names()
            .flatten()
            .find_map(|n| caps.name(n).map(|g| g.as_str().trim().to_string()))
            .unwrap_or_else(|| m.as_str().trim().to_string());
        if name.is_empty() || NOT_FUNCTIONS.contains(&name.as_str()) {
            continue;
        }
        // The name's position determines the line.
        let pos = re.capture_names().flatten().find_map(|n| caps.name(n).map(|g| g.start())).unwrap_or(m.start());
        line += text[last..pos].bytes().filter(|b| *b == b'\n').count();
        last = pos;
        out.push(FunctionItem { name: name.into(), line });
        if out.len() > 20_000 {
            break;
        }
    }
    out
}

impl Render for FunctionListPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        if self.filter.is_none() {
            let f = cx.new(|cx| Editor::single_line("Search", window, cx));
            cx.subscribe(&f, |_, _, ev: &EditorEvent, cx| {
                if matches!(ev, EditorEvent::Edited) {
                    cx.notify();
                }
            })
            .detach();
            self.filter = Some(f);
        }
        let filter = self.filter.clone().unwrap();
        let q = filter.read(cx).text(cx).to_lowercase();
        let mut items: Vec<FunctionItem> = self.items.iter().filter(|i| q.is_empty() || i.name.to_lowercase().contains(&q)).cloned().collect();
        if self.sort_alpha {
            items.sort_by_key(|i| i.name.to_lowercase());
        }
        let ws = self.workspace.clone();
        let t = theme.clone();
        let caret_line = ws
            .upgrade()
            .and_then(|w| w.read(cx).active_editor())
            .map(|e| {
                let ed = e.read(cx);
                ed.buf(cx).char_to_line(ed.caret())
            })
            .unwrap_or(0);
        let current = self.items.iter().rev().find(|i| i.line <= caret_line).map(|i| i.line);
        let n = items.len();
        let list = uniform_list("func-list", n, move |range, _, _| {
            range
                .map(|ix| {
                    let item = items[ix].clone();
                    let ws = ws.clone();
                    let is_cur = current == Some(item.line);
                    div()
                        .id(("func", ix))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .h(px(21.0))
                        .px(px(8.0))
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .text_size(px(12.5))
                        .when(is_cur, |d| d.bg(t.list_selected))
                        .hover(|d| d.bg(t.list_hover))
                        .child(ui::icon("chevron-right", 10.0, t.accent))
                        .child(item.name.clone())
                        .child(div().text_color(t.muted_fg).text_size(px(11.0)).child(format!(":{}", item.line + 1)))
                        .on_click(move |_, window, cx| {
                            if let Some(ws) = ws.upgrade() {
                                let e = ws.read(cx).active_editor();
                                if let Some(e) = e {
                                    e.update(cx, |e, cx| e.go_to_line(item.line, cx));
                                    window.focus(&e.focus_handle(cx));
                                }
                            }
                        })
                })
                .collect()
        })
        .track_scroll(self.scroll.clone())
        .flex_1();
        let ws = self.workspace.clone();
        let lang = crate::syntax::display_name(&self.language);
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(2.0))
                    .h(px(26.0))
                    .px(px(8.0))
                    .bg(theme.panel_header_bg)
                    .border_b_1()
                    .border_color(theme.chrome_border)
                    .child(div().flex_1().text_size(px(12.0)).font_weight(gpui::FontWeight::SEMIBOLD).child("Function List"))
                    .child(ui::icon_button("fl-sort", "list", &theme, self.sort_alpha).tooltip(ui::tooltip("Sort alphabetically")).on_click(cx.listener(|this, _, _, cx| {
                        this.sort_alpha = !this.sort_alpha;
                        cx.notify();
                    })))
                    .child(ui::icon_button("fl-refresh", "refresh", &theme, false).tooltip(ui::tooltip("Reload")).on_click(cx.listener(|this, _, _, cx| this.schedule_refresh(cx))))
                    .child(ui::icon_button("fl-close", "x", &theme, false).on_click(move |_, window, cx| {
                        if let Some(ws) = ws.upgrade() {
                            ws.update(cx, |ws, cx| {
                                ws.right_panel = None;
                                ws.focus_active(window, cx);
                                cx.notify();
                            });
                        }
                    })),
            )
            .child(div().p(px(6.0)).child(ui::input(&filter, &theme, cx)))
            .when(n == 0, |d| {
                d.child(div().px(px(10.0)).child(ui::muted(
                    if crate::syntax::function_list_pattern(&self.language).is_none() {
                        format!("No function list parser for {lang}.")
                    } else {
                        "No functions found.".to_string()
                    },
                    &theme,
                )))
            })
            .child(list)
    }
}

// ====================================================================== Minimap

#[derive(Clone, Copy, Default)]
struct MapGeom {
    top_line: f32,
    line_h: f32,
    origin_y: f32,
    lines: usize,
}

pub struct DocMapPanel {
    workspace: WeakEntity<Workspace>,
    geom: Rc<Cell<MapGeom>>,
    dragging: bool,
}

impl DocMapPanel {
    pub fn new(workspace: WeakEntity<Workspace>, _cx: &mut Context<Self>) -> Self {
        DocMapPanel { workspace, geom: Rc::new(Cell::new(MapGeom::default())), dragging: false }
    }

    fn scroll_to(&mut self, y: gpui::Pixels, cx: &mut Context<Self>) {
        let g = self.geom.get();
        if g.line_h <= 0.0 {
            return;
        }
        let line = (g.top_line + (f32::from(y) - g.origin_y) / g.line_h).max(0.0) as usize;
        let line = line.min(g.lines.saturating_sub(1));
        let Some(ws) = self.workspace.upgrade() else { return };
        let Some(e) = ws.read(cx).active_editor() else { return };
        e.update(cx, |e, cx| {
            let (lh, h) = e.layout.as_ref().map(|l| (l.line_height, f32::from(l.text_bounds.size.height))).unwrap_or((18.0, 600.0));
            let row = e.display.line_to_row(line) as f32;
            e.set_scroll(None, Some(row * lh - h / 2.0), cx);
            cx.emit(EditorEvent::Scrolled);
        });
    }
}

impl Render for DocMapPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = crate::theme::theme(cx);
        let ws = self.workspace.clone();
        let geom = self.geom.clone();
        let t = theme.clone();
        let ws_close = self.workspace.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(panel_header("Minimap", &theme, move |_, window, cx| {
                if let Some(ws) = ws_close.upgrade() {
                    ws.update(cx, |ws, cx| {
                        ws.right_panel = None;
                        ws.focus_active(window, cx);
                        cx.notify();
                    });
                }
            }))
            .child(
                div()
                    .id("docmap")
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .bg(theme.bg)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        this.dragging = true;
                        this.scroll_to(e.position.y, cx);
                    }))
                    .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, _| this.dragging = false))
                    .on_mouse_move(cx.listener(|this, e: &MouseMoveEvent, _, cx| {
                        if this.dragging && e.pressed_button == Some(MouseButton::Left) {
                            this.scroll_to(e.position.y, cx);
                        } else {
                            this.dragging = false;
                        }
                    }))
                    .child(
                        canvas(
                            move |bounds, _, cx| map_quads(&ws, bounds, &t, &geom, cx),
                            |_, quads, window, _| {
                                for q in quads {
                                    window.paint_quad(q);
                                }
                            },
                        )
                        .size_full(),
                    ),
            )
    }
}

fn map_quads(ws: &WeakEntity<Workspace>, bounds: Bounds<gpui::Pixels>, theme: &AppTheme, geom: &Rc<Cell<MapGeom>>, cx: &mut gpui::App) -> Vec<gpui::PaintQuad> {
    let mut quads = Vec::new();
    let Some(ws) = ws.upgrade() else { return quads };
    let Some(e) = ws.read(cx).active_editor() else { return quads };
    let ed = e.read(cx);
    let b = ed.buf(cx);
    let n = b.len_lines();
    let line_h = 2.5f32;
    let char_w = 1.1f32;
    let h = f32::from(bounds.size.height);
    let w = f32::from(bounds.size.width);
    let (lh, view_h) = ed.layout.as_ref().map(|l| (l.line_height, f32::from(l.text_bounds.size.height))).unwrap_or((18.0, 600.0));
    let first_row = (ed.scroll_y / lh) as usize;
    let (first_line, _) = ed.display.row_to_line(first_row);
    let visible_lines = (view_h / lh).ceil().max(1.0);
    let map_lines = (h / line_h).floor();
    let top_line = if (n as f32) <= map_lines {
        0.0
    } else {
        let max_first = (n as f32 - visible_lines).max(1.0);
        let frac = (first_line as f32 / max_first).clamp(0.0, 1.0);
        frac * (n as f32 - map_lines)
    };
    geom.set(MapGeom { top_line, line_h, origin_y: f32::from(bounds.origin.y), lines: n });
    let ox = f32::from(bounds.origin.x) + 4.0;
    let oy = f32::from(bounds.origin.y);
    let start = top_line as usize;
    let end = (start + map_lines as usize + 1).min(n);
    let max_chars = ((w - 8.0) / char_w) as usize;
    let tab = cx.global::<crate::settings::Settings>().tab_size.max(1);
    for line in start..end {
        let y = oy + (line - start) as f32 * line_h;
        let text = b.line_text(line);
        let spans = b.highlighter.get(line);
        let mut col = 0usize;
        let mut ci = 0usize;
        let mut span_iter = spans.as_ref().map(|s| s.iter().copied().collect::<Vec<_>>()).unwrap_or_default().into_iter();
        let mut cur_span = span_iter.next();
        let mut span_left = cur_span.map(|s| s.len as usize).unwrap_or(usize::MAX);
        let mut run_start: Option<(usize, gpui::Hsla)> = None;
        for c in text.chars() {
            let color = cur_span.map(|s| crate::theme::rgb_hex(s.color)).unwrap_or(theme.fg);
            let width = if c == '\t' { tab - col % tab } else { 1 };
            if c.is_whitespace() {
                if let Some((s, colr)) = run_start.take() {
                    let mut cc = colr;
                    cc.a = 0.55;
                    quads.push(fill(Bounds::new(point(px(ox + s as f32 * char_w), px(y)), size(px((col - s) as f32 * char_w), px(line_h - 0.6))), cc));
                }
            } else {
                match run_start {
                    Some((_, rc)) if rc == color => {}
                    Some((s, rc)) => {
                        let mut cc = rc;
                        cc.a = 0.55;
                        quads.push(fill(Bounds::new(point(px(ox + s as f32 * char_w), px(y)), size(px((col - s) as f32 * char_w), px(line_h - 0.6))), cc));
                        run_start = Some((col, color));
                    }
                    None => run_start = Some((col, color)),
                }
            }
            col += width;
            ci += 1;
            if span_left != usize::MAX {
                span_left -= 1;
                if span_left == 0 {
                    cur_span = span_iter.next();
                    span_left = cur_span.map(|s| s.len as usize).unwrap_or(usize::MAX);
                }
            }
            if col >= max_chars {
                break;
            }
        }
        let _ = ci;
        if let Some((s, rc)) = run_start {
            let mut cc = rc;
            cc.a = 0.55;
            quads.push(fill(Bounds::new(point(px(ox + s as f32 * char_w), px(y)), size(px((col - s) as f32 * char_w), px(line_h - 0.6))), cc));
        }
    }
    // Viewport.
    let vy = oy + (first_line as f32 - top_line) * line_h;
    let mut vc = theme.accent;
    vc.a = 0.16;
    quads.push(fill(Bounds::new(point(bounds.origin.x, px(vy)), size(bounds.size.width, px(visible_lines * line_h))), vc));
    let mut border = theme.accent;
    border.a = 0.5;
    quads.push(fill(Bounds::new(point(bounds.origin.x, px(vy)), size(bounds.size.width, px(1.0))), border));
    quads.push(fill(Bounds::new(point(bounds.origin.x, px(vy + visible_lines * line_h)), size(bounds.size.width, px(1.0))), border));
    quads
}

// ====================================================================== Character panel

const HTML_NAMES: &[(u32, &str)] = &[
    (34, "&quot;"), (38, "&amp;"), (60, "&lt;"), (62, "&gt;"), (160, "&nbsp;"), (161, "&iexcl;"), (162, "&cent;"),
    (163, "&pound;"), (164, "&curren;"), (165, "&yen;"), (166, "&brvbar;"), (167, "&sect;"), (168, "&uml;"),
    (169, "&copy;"), (170, "&ordf;"), (171, "&laquo;"), (172, "&not;"), (173, "&shy;"), (174, "&reg;"),
    (175, "&macr;"), (176, "&deg;"), (177, "&plusmn;"), (178, "&sup2;"), (179, "&sup3;"), (180, "&acute;"),
    (181, "&micro;"), (182, "&para;"), (183, "&middot;"), (184, "&cedil;"), (185, "&sup1;"), (186, "&ordm;"),
    (187, "&raquo;"), (188, "&frac14;"), (189, "&frac12;"), (190, "&frac34;"), (191, "&iquest;"), (215, "&times;"),
    (247, "&divide;"),
];

pub fn render_char_panel(ws: &Workspace, theme: &AppTheme, cx: &mut Context<Workspace>) -> AnyElement {
    let weak = ws.weak_self.clone();
    let t = theme.clone();
    let list = uniform_list("char-panel", 256, move |range, _, _| {
        range
            .map(|v| {
                let ch = if v < 128 {
                    char::from_u32(v as u32).unwrap_or(' ')
                } else {
                    // Show the Windows-1252 ("ANSI") interpretation of bytes 128-255.
                    let byte = [v as u8];
                    let (s, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&byte);
                    let c = s.chars().next().unwrap_or(' ');
                    c
                };
                let shown = if (v as u32) < 32 {
                    crate::editor::display::CONTROL_NAMES[v].to_string()
                } else if v == 127 {
                    "DEL".into()
                } else {
                    ch.to_string()
                };
                let html = HTML_NAMES.iter().find(|(c, _)| *c == ch as u32).map(|(_, n)| n.to_string()).unwrap_or_default();
                let weak = weak.clone();
                div()
                    .id(("char", v))
                    .flex()
                    .items_center()
                    .h(px(20.0))
                    .px(px(8.0))
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .hover(|d| d.bg(t.list_hover))
                    .child(div().w(px(36.0)).child(v.to_string()))
                    .child(div().w(px(36.0)).text_color(t.muted_fg).child(format!("{v:02X}")))
                    .child(div().w(px(44.0)).font_family("Menlo").child(shown))
                    .child(div().w(px(60.0)).text_color(t.muted_fg).child(format!("&#{};", ch as u32)))
                    .child(div().text_color(t.muted_fg).child(html))
                    .on_click(move |_, window, cx| {
                        let Some(ws) = weak.upgrade() else { return };
                        if let Some(e) = ws.read(cx).active_editor() {
                            e.update(cx, |e, cx| e.insert_text(&ch.to_string(), window, cx));
                            window.focus(&e.focus_handle(cx));
                        }
                    })
            })
            .collect()
    })
    .flex_1();
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(panel_header("Character Codes", theme, cx.listener(|this, _, window, cx| {
            this.right_panel = None;
            this.focus_active(window, cx);
            cx.notify();
        })))
        .child(
            div()
                .flex()
                .h(px(22.0))
                .px(px(8.0))
                .items_center()
                .text_size(px(11.0))
                .text_color(theme.muted_fg)
                .border_b_1()
                .border_color(theme.chrome_border)
                .child(div().w(px(36.0)).child("Value"))
                .child(div().w(px(36.0)).child("Hex"))
                .child(div().w(px(44.0)).child("Char"))
                .child(div().w(px(60.0)).child("HTML #"))
                .child(div().child("HTML name")),
        )
        .child(list)
        .into_any_element()
}

// ====================================================================== Clipboard history

pub fn render_clipboard_panel(ws: &Workspace, theme: &AppTheme, cx: &mut Context<Workspace>) -> AnyElement {
    let items: Vec<String> = cx.try_global::<crate::editor::ClipboardHistory>().map(|h| h.items.clone()).unwrap_or_default();
    let weak = ws.weak_self.clone();
    let rows: Vec<AnyElement> = items
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let preview: String = text.lines().next().unwrap_or("").chars().take(80).collect();
            let more = text.lines().count() > 1;
            let text = text.clone();
            let weak = weak.clone();
            div()
                .id(("clip", i))
                .px(px(8.0))
                .py(px(4.0))
                .border_b_1()
                .border_color(theme.chrome_border)
                .text_size(px(12.0))
                .font_family("Menlo")
                .whitespace_nowrap()
                .overflow_hidden()
                .cursor_pointer()
                .hover(|d| d.bg(theme.list_hover))
                .child(format!("{preview}{}", if more { " …" } else { "" }))
                .tooltip(ui::tooltip(text.chars().take(400).collect::<String>()))
                .on_click(move |_, window, cx| {
                    let Some(ws) = weak.upgrade() else { return };
                    if let Some(e) = ws.read(cx).active_editor() {
                        e.update(cx, |e, cx| e.paste_text(&text, "", window, cx));
                        window.focus(&e.focus_handle(cx));
                    }
                })
                .into_any_element()
        })
        .collect();
    div()
        .size_full()
        .flex()
        .flex_col()
        .child(panel_header("Clipboard History", theme, cx.listener(|this, _, window, cx| {
            this.right_panel = None;
            this.focus_active(window, cx);
            cx.notify();
        })))
        .when(rows.is_empty(), |d| d.child(div().p(px(10.0)).child(ui::muted("Copy or cut text to collect it here.", theme))))
        .child(div().id("clip-list").flex_1().overflow_y_scroll().children(rows))
        .into_any_element()
}
