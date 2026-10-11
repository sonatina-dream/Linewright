//! The main window: tab bars, split views, panels, dialogs, status bar and all menu commands.

mod chrome;
pub mod dialogs;
mod files;
pub mod find_panel;
mod commands;
pub mod panels;
pub mod results;
mod search_ops;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    prelude::*, Action, AnyView, App, Context, DismissEvent, Entity, EntityId, FocusHandle, Focusable,
    ManagedView, Pixels, Point, SharedString, Subscription, WeakEntity, Window,
};

use crate::buffer::Buffer;
use crate::editor::{Editor, EditorEvent, EditorMode};
use crate::session::AppState;
use crate::settings::Settings;

pub use find_panel::FindPanel;
pub use results::ResultsPanel;

pub struct Pane {
    pub items: Vec<Entity<Editor>>,
    pub active: usize,
}

impl Pane {
    pub fn active_item(&self) -> Option<&Entity<Editor>> {
        self.items.get(self.active)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeftPanel {
    Folder,
    DocList,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RightPanel {
    FunctionList,
    DocMap,
    CharPanel,
    Clipboard,
}

pub struct MenuEntry {
    pub label: SharedString,
    pub action: Option<Box<dyn Action>>,
    pub callback: Option<Rc<dyn Fn(&mut Workspace, &mut Window, &mut Context<Workspace>)>>,
    pub checked: bool,
    pub enabled: bool,
    pub separator: bool,
}

impl MenuEntry {
    pub fn action(label: impl Into<SharedString>, action: impl Action) -> Self {
        MenuEntry { label: label.into(), action: Some(Box::new(action)), callback: None, checked: false, enabled: true, separator: false }
    }
    pub fn callback(label: impl Into<SharedString>, f: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static) -> Self {
        MenuEntry { label: label.into(), action: None, callback: Some(Rc::new(f)), checked: false, enabled: true, separator: false }
    }
    pub fn separator() -> Self {
        MenuEntry { label: SharedString::default(), action: None, callback: None, checked: false, enabled: false, separator: true }
    }
    pub fn checked(mut self, c: bool) -> Self {
        self.checked = c;
        self
    }
    pub fn enabled(mut self, e: bool) -> Self {
        self.enabled = e;
        self
    }
}

pub struct ContextMenu {
    pub position: Point<Pixels>,
    pub entries: Vec<MenuEntry>,
    pub min_width: f32,
}

pub struct Workspace {
    pub panes: Vec<Pane>,
    pub active_pane: usize,
    pub focus_handle: FocusHandle,
    pub weak_self: WeakEntity<Workspace>,
    pub next_buffer_id: u64,
    editor_subs: HashMap<EntityId, Vec<Subscription>>,
    pub find_panel: Entity<FindPanel>,
    pub find_visible: bool,
    pub find_pos: Option<Point<Pixels>>,
    pub incremental_sub: Option<Subscription>,
    pub results: Entity<ResultsPanel>,
    pub results_visible: bool,
    pub left_panel: Option<LeftPanel>,
    pub right_panel: Option<RightPanel>,
    pub folder: Entity<panels::FolderPanel>,
    pub function_list: Entity<panels::FunctionListPanel>,
    pub doc_map: Entity<panels::DocMapPanel>,
    pub modal: Option<(AnyView, FocusHandle, Subscription)>,
    pub context_menu: Option<ContextMenu>,
    pub state: AppState,
    pub closed_files: Vec<PathBuf>,
    pub notification: Option<(SharedString, Instant)>,
    pub sync_vertical: bool,
    pub sync_horizontal: bool,
    pub incremental: Option<Entity<Editor>>,
    pub incremental_status: SharedString,
    pub split_ratio: f32,
    pub split_vertical: bool,
    pub menus_signature: String,
    pub focus_mode: bool,
    pub floating_note: bool,
    pub always_on_top: bool,
    pub last_title: String,
    pub last_dark: Option<bool>,
    pub window_bounds: Option<(f32, f32, f32, f32)>,
    pub dirty_cache: HashMap<u64, bool>,
    pub pinned: std::collections::HashSet<EntityId>,
    pub quitting: bool,
    pub tab_bar_scroll: [gpui::ScrollHandle; 2],
    pub compare_active: bool,
    pub prompting: bool,
    pub session_dirty: bool,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let weak = cx.weak_entity();
        let find_panel = cx.new(|cx| FindPanel::new(weak.clone(), window, cx));
        let results = cx.new(|cx| ResultsPanel::new(weak.clone(), cx));
        let folder = cx.new(|cx| panels::FolderPanel::new(weak.clone(), window, cx));
        let function_list = cx.new(|cx| panels::FunctionListPanel::new(weak.clone(), cx));
        let doc_map = cx.new(|cx| panels::DocMapPanel::new(weak.clone(), cx));
        let settings_sub = cx.observe_global_in::<Settings>(window, |this, window, cx| {
            this.refresh_menus(window, cx);
            cx.notify();
        });
        let theme_sub = cx.observe_global::<crate::theme::ThemeState>(|_, cx| cx.notify());
        let state = AppState::load();
        let split_vertical = cx.global::<Settings>().split_vertical;
        let mut ws = Workspace {
            panes: vec![Pane { items: Vec::new(), active: 0 }, Pane { items: Vec::new(), active: 0 }],
            active_pane: 0,
            focus_handle: cx.focus_handle(),
            weak_self: weak,
            next_buffer_id: 1,
            editor_subs: HashMap::new(),
            find_panel,
            find_visible: false,
            find_pos: None,
            incremental_sub: None,
            results,
            results_visible: false,
            left_panel: None,
            right_panel: None,
            folder,
            function_list,
            doc_map,
            modal: None,
            context_menu: None,
            state,
            closed_files: Vec::new(),
            notification: None,
            sync_vertical: false,
            sync_horizontal: false,
            incremental: None,
            incremental_status: SharedString::default(),
            split_ratio: 0.5,
            split_vertical,
            menus_signature: String::new(),
            focus_mode: false,
            floating_note: false,
            always_on_top: false,
            last_title: String::new(),
            last_dark: None,
            window_bounds: None,
            dirty_cache: HashMap::new(),
            pinned: Default::default(),
            quitting: false,
            tab_bar_scroll: [gpui::ScrollHandle::new(), gpui::ScrollHandle::new()],
            compare_active: false,
            prompting: false,
            session_dirty: false,
            _subscriptions: vec![settings_sub, theme_sub],
        };
        ws.start_background_tasks(window, cx);
        ws
    }

    // ------------------------------------------------------------------ accessors

    pub fn active_editor(&self) -> Option<Entity<Editor>> {
        self.panes.get(self.active_pane).and_then(|p| p.active_item().cloned()).or_else(|| self.panes[0].active_item().cloned())
    }

    pub fn other_pane(&self) -> usize {
        1 - self.active_pane
    }

    pub fn is_split(&self) -> bool {
        !self.panes[1].items.is_empty()
    }

    pub fn all_editors(&self) -> Vec<Entity<Editor>> {
        self.panes.iter().flat_map(|p| p.items.iter().cloned()).collect()
    }

    /// One editor per distinct buffer.
    pub fn unique_buffers(&self, cx: &App) -> Vec<(Entity<Editor>, Entity<Buffer>)> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for e in self.all_editors() {
            let b = e.read(cx).buffer.clone();
            if seen.insert(b.entity_id()) {
                out.push((e, b));
            }
        }
        out
    }

    pub fn find_editor_location(&self, editor: &Entity<Editor>) -> Option<(usize, usize)> {
        for (pi, p) in self.panes.iter().enumerate() {
            if let Some(i) = p.items.iter().position(|e| e == editor) {
                return Some((pi, i));
            }
        }
        None
    }

    pub fn editor_for_path(&self, path: &std::path::Path, cx: &App) -> Option<(usize, usize)> {
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        for (pi, p) in self.panes.iter().enumerate() {
            for (i, e) in p.items.iter().enumerate() {
                let f = e.read(cx).buf(cx).file.clone();
                if f.as_deref() == Some(path) || f.as_deref() == Some(canon.as_path()) {
                    return Some((pi, i));
                }
            }
        }
        None
    }

    pub fn notify_message(&mut self, msg: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.notification = Some((msg.into(), Instant::now()));
        cx.notify();
        let weak = self.weak_self.clone();
        cx.spawn(async move |_, cx| {
            cx.background_executor().timer(Duration::from_millis(4200)).await;
            let _ = weak.update(cx, |this, cx| {
                if let Some((_, t)) = &this.notification {
                    if t.elapsed() >= Duration::from_millis(4000) {
                        this.notification = None;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    // ------------------------------------------------------------------ editors

    pub fn alloc_buffer_id(&mut self) -> u64 {
        let id = self.next_buffer_id;
        self.next_buffer_id += 1;
        id
    }

    pub fn next_untitled_name(&mut self, cx: &App) -> String {
        let used: std::collections::HashSet<String> = self
            .all_editors()
            .iter()
            .map(|e| e.read(cx).buf(cx).untitled_name.clone())
            .collect();
        let mut n = 1;
        loop {
            let name = format!("new {n}");
            if !used.contains(&name) {
                return name;
            }
            n += 1;
        }
    }

    /// Create an editor for `buffer` and add it to `pane`.
    pub fn add_buffer(&mut self, buffer: Entity<Buffer>, pane: usize, activate: bool, window: &mut Window, cx: &mut Context<Self>) -> Entity<Editor> {
        let editor = cx.new(|cx| Editor::new(buffer.clone(), EditorMode::Full, window, cx));
        self.attach_editor(&editor, window, cx);
        let p = &mut self.panes[pane];
        let insert_at = if p.items.is_empty() { 0 } else { p.active + 1 };
        p.items.insert(insert_at.min(p.items.len()), editor.clone());
        if activate {
            p.active = insert_at.min(p.items.len() - 1);
            self.active_pane = pane;
            window.focus(&editor.focus_handle(cx));
        } else if p.items.len() == 1 {
            p.active = 0;
        }
        self.on_active_changed(window, cx);
        editor
    }

    fn attach_editor(&mut self, editor: &Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) {
        let sub = cx.subscribe_in(editor, window, Self::on_editor_event);
        let buffer = editor.read(cx).buffer.clone();
        let bsub = cx.observe(&buffer, |this, buffer, cx| {
            let id = buffer.read(cx).id;
            let dirty = buffer.read(cx).is_dirty();
            if this.dirty_cache.insert(id, dirty) != Some(dirty) {
                cx.notify();
            }
        });
        self.editor_subs.insert(editor.entity_id(), vec![sub, bsub]);
    }

    fn on_editor_event(&mut self, editor: &Entity<Editor>, ev: &EditorEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            EditorEvent::SelectionsChanged => {
                cx.notify();
            }
            EditorEvent::Edited => {
                if self.active_editor().as_ref() == Some(editor) {
                    self.function_list.update(cx, |f, cx| f.schedule_refresh(cx));
                    self.doc_map.update(cx, |_, cx| cx.notify());
                }
                cx.notify();
            }
            EditorEvent::Focused => {
                if let Some((pi, i)) = self.find_editor_location(editor) {
                    let changed = self.active_pane != pi || self.panes[pi].active != i;
                    self.active_pane = pi;
                    self.panes[pi].active = i;
                    if changed {
                        self.on_active_changed(window, cx);
                    }
                }
                cx.notify();
            }
            EditorEvent::ContextMenu(pos) => {
                self.show_editor_context_menu(*pos, cx);
            }
            EditorEvent::Zoom(dir) => {
                if *dir > 0 {
                    self.zoom_in(&crate::actions::ZoomIn, window, cx);
                } else {
                    self.zoom_out(&crate::actions::ZoomOut, window, cx);
                }
            }
            EditorEvent::Scrolled => {
                self.sync_scroll_from(editor, cx);
                self.doc_map.update(cx, |_, cx| cx.notify());
            }
            EditorEvent::Notify(msg) => self.notify_message(msg.clone(), cx),
            _ => {}
        }
    }

    fn sync_scroll_from(&mut self, editor: &Entity<Editor>, cx: &mut Context<Self>) {
        if !(self.sync_vertical || self.sync_horizontal) || !self.is_split() {
            return;
        }
        let Some((pi, _)) = self.find_editor_location(editor) else { return };
        let Some(other) = self.panes[1 - pi].active_item().cloned() else { return };
        if &other == editor {
            return;
        }
        let (sx, sy) = {
            let e = editor.read(cx);
            (e.scroll_x, e.scroll_y)
        };
        let (v, h) = (self.sync_vertical, self.sync_horizontal);
        other.update(cx, |o, cx| {
            let changed = (v && (o.scroll_y - sy).abs() > 0.5) || (h && (o.scroll_x - sx).abs() > 0.5);
            if changed {
                o.set_scroll(if h { Some(sx) } else { None }, if v { Some(sy) } else { None }, cx);
            }
        });
    }

    /// Called whenever the active document changes.
    pub fn on_active_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.function_list.update(cx, |f, cx| f.schedule_refresh(cx));
        self.doc_map.update(cx, |_, cx| cx.notify());
        self.refresh_menus(window, cx);
        cx.notify();
    }

    pub fn activate(&mut self, pane: usize, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.panes.get(pane).and_then(|p| p.items.get(index)).cloned() {
            self.active_pane = pane;
            self.panes[pane].active = index;
            window.focus(&e.focus_handle(cx));
            self.tab_bar_scroll[pane].scroll_to_item(index);
            self.on_active_changed(window, cx);
        }
    }

    pub fn focus_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = self.active_editor() {
            window.focus(&e.focus_handle(cx));
        }
    }

    /// Remove an editor without any prompting.
    pub fn remove_editor(&mut self, pane: usize, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.panes[pane].items.len() {
            return;
        }
        let editor = self.panes[pane].items.remove(index);
        self.editor_subs.remove(&editor.entity_id());
        self.pinned.remove(&editor.entity_id());
        let (path, backup, buffer_eid) = {
            let e = editor.read(cx);
            let b = e.buf(cx);
            (b.file.clone(), b.backup_file.clone(), e.buffer.entity_id())
        };
        if let Some(p) = path {
            self.closed_files.retain(|c| *c != p);
            self.closed_files.push(p);
        }
        // Delete the snapshot if no other view uses this buffer.
        let still_open = self.all_editors().iter().any(|e| e.read(cx).buffer.entity_id() == buffer_eid);
        if !still_open {
            if let Some(bk) = backup {
                let _ = std::fs::remove_file(bk);
            }
        }
        self.session_dirty = true;
        let p = &mut self.panes[pane];
        if p.active >= p.items.len() && !p.items.is_empty() {
            p.active = p.items.len() - 1;
        } else if index < p.active {
            p.active -= 1;
        }
        if self.panes[0].items.is_empty() {
            if !self.panes[1].items.is_empty() {
                // Promote the sub view to the main view.
                self.panes.swap(0, 1);
                self.active_pane = 0;
            } else if !self.quitting {
                self.new_file(&crate::actions::NewFile, window, cx);
                return;
            }
        }
        if self.panes[self.active_pane].items.is_empty() {
            self.active_pane = 0;
        }
        self.focus_active(window, cx);
        self.on_active_changed(window, cx);
    }

    // ------------------------------------------------------------------ modal + menus

    pub fn open_modal<V: ManagedView>(&mut self, view: Entity<V>, window: &mut Window, cx: &mut Context<Self>) {
        let focus = view.focus_handle(cx);
        let sub = cx.subscribe_in(&view, window, |this, _, _: &DismissEvent, window, cx| {
            this.close_modal(window, cx);
        });
        window.focus(&focus);
        self.modal = Some((view.into(), focus, sub));
        self.context_menu = None;
        cx.notify();
    }

    pub fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal.take().is_some() {
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    pub fn show_context_menu(&mut self, position: Point<Pixels>, entries: Vec<MenuEntry>, cx: &mut Context<Self>) {
        self.context_menu = Some(ContextMenu { position, entries, min_width: 200.0 });
        cx.notify();
    }

    pub fn run_menu_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.context_menu.take() else { return };
        let Some(entry) = menu.entries.into_iter().nth(index) else { return };
        if !entry.enabled {
            return;
        }
        if let Some(cb) = entry.callback {
            cb(self, window, cx);
        } else if let Some(action) = entry.action {
            self.focus_active(window, cx);
            window.dispatch_action(action, cx);
        }
        cx.notify();
    }

    pub fn refresh_menus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sig = crate::menus::signature(self, cx);
        if sig != self.menus_signature {
            self.menus_signature = sig;
            let menus = crate::menus::build(self, cx);
            cx.set_menus(menus);
            crate::platform::apply_menu_checkmarks();
        }
        let _ = window;
    }

    // ------------------------------------------------------------------ background tasks

    fn start_background_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // File change detection + periodic backups.
        cx.spawn_in(window, async move |this, cx| {
            let mut tick: u64 = 0;
            loop {
                cx.background_executor().timer(Duration::from_millis(1500)).await;
                tick += 1;
                let alive = this
                    .update_in(cx, |this, window, cx| {
                        if window.is_window_active() || this.any_monitoring(cx) {
                            this.check_files_changed(window, cx);
                        }
                        let interval = cx.global::<Settings>().backup_interval_secs.max(2);
                        if (tick * 1500 / 1000) % interval < 2 && cx.global::<Settings>().session_snapshot {
                            this.write_backups(cx);
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        })
        .detach();
    }

    fn any_monitoring(&self, cx: &App) -> bool {
        self.all_editors().iter().any(|e| e.read(cx).buf(cx).monitoring)
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_workspace(window, cx)
    }
}

