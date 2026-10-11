//! Linewright — a fast, native text and source-code editor for macOS, written in Rust with GPUI.

// The `objc` crate's `msg_send!` emits `cfg(feature = "cargo-clippy")`.
#![allow(unexpected_cfgs)]

mod actions;
mod assets;
mod buffer;
mod debug;
mod editor;
mod encoding;
mod keymap;
mod menus;
mod platform;
mod search;
mod session;
mod settings;
mod syntax;
mod theme;
mod tools;
mod ui;
mod workspace;

use std::path::PathBuf;

use futures::StreamExt;
use gpui::{
    point, px, size, App, AppContext as _, Application, Bounds, Focusable, TitlebarOptions, WindowBounds, WindowHandle,
    WindowOptions,
};

use crate::settings::Settings;
use crate::workspace::Workspace;

/// Application name shown in the UI.
pub const APP_NAME: &str = "Linewright";
/// macOS bundle identifier (keep in sync with packaging/Info.plist).
pub const APP_ID: &str = "io.github.sonatina-dream.linewright";
/// Project home page (Help menu).
pub const HOMEPAGE: &str = "https://github.com/sonatina-dream/linewright";

fn paths_from_args() -> (Vec<PathBuf>, bool) {
    let mut paths = Vec::new();
    let mut new_instance = false;
    for a in std::env::args().skip(1) {
        if a == "--new-instance" {
            new_instance = true;
        } else if a.starts_with("-psn_") || a.starts_with('-') {
            continue;
        } else {
            let p = PathBuf::from(&a);
            let p = if p.is_relative() { std::env::current_dir().map(|d| d.join(&p)).unwrap_or(p) } else { p };
            paths.push(p);
        }
    }
    (paths, new_instance)
}

fn url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    // Strict percent-decoding: '+' is a literal plus in file URLs.
    let decoded = percent_encoding::percent_decode_str(rest).decode_utf8_lossy().into_owned();
    Some(PathBuf::from(decoded))
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_urls_keep_plus_signs() {
        assert_eq!(
            super::url_to_path("file:///Users/me/C++%20notes/a+b%20c.txt"),
            Some(std::path::PathBuf::from("/Users/me/C++ notes/a+b c.txt"))
        );
    }
}

fn open_main_window(cx: &mut App, paths: Vec<PathBuf>, restore: bool) -> Option<WindowHandle<Workspace>> {
    let session = if restore { session::Session::load() } else { None };
    let bounds = session
        .as_ref()
        .and_then(|s| s.window)
        .map(|(x, y, w, h)| Bounds::new(point(px(x), px(y)), size(px(w.max(400.0)), px(h.max(300.0)))))
        .unwrap_or_else(|| Bounds::centered(None, size(px(1280.0), px(820.0)), cx));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions { title: Some("Linewright".into()), appears_transparent: false, traffic_light_position: None }),
        window_min_size: Some(size(px(480.0), px(320.0))),
        app_id: Some(crate::APP_ID.into()),
        ..Default::default()
    };
    let handle = cx
        .open_window(options, |window, cx| {
            let ws = cx.new(|cx| Workspace::new(window, cx));
            ws.update(cx, |ws, cx| {
                let remember = cx.global::<Settings>().remember_session || cx.global::<Settings>().session_snapshot;
                if let (Some(s), true) = (&session, remember) {
                    ws.restore_session(s, window, cx);
                }
                if !paths.is_empty() {
                    ws.open_paths(paths.clone(), window, cx);
                }
                if ws.panes[0].items.is_empty() {
                    ws.new_file(&actions::NewFile, window, cx);
                }
                ws.refresh_menus(window, cx);
                ws.focus_active(window, cx);
            });
            // Closing the main window quits the editor (with the usual save prompts).
            let ws2 = ws.clone();
            window.on_window_should_close(cx, move |window, cx| {
                ws2.update(cx, |ws, cx| {
                    if ws.quitting {
                        return true;
                    }
                    let task = ws.prepare_quit(window, cx);
                    cx.spawn(async move |_, cx| {
                        if task.await {
                            let _ = cx.update(|cx| cx.quit());
                        }
                    })
                    .detach();
                    false
                })
            });
            ws
        })
        .ok()?;
    Some(handle)
}

fn main() {
    let (paths, new_instance) = paths_from_args();
    if new_instance {
        session::SESSION_DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let (url_tx, mut url_rx) = futures::channel::mpsc::unbounded::<Vec<String>>();
    let app = Application::new().with_assets(assets::Assets);
    app.on_open_urls(move |urls| {
        let _ = url_tx.unbounded_send(urls);
    });
    app.run(move |cx: &mut App| {
        let settings = Settings::load();
        let theme_name = settings.theme.clone();
        cx.set_global(settings);
        theme::init(cx, &theme_name);
        cx.set_global(editor::MacroRecorder::default());
        cx.set_global(editor::ClipboardHistory::default());
        // Warm up the syntax set off the main thread.
        cx.background_executor().spawn(async { syntax::syntax_set(); }).detach();

        keymap::reload_keymap(cx);

        cx.on_action(|_: &actions::Quit, cx: &mut App| cx.quit());
        cx.on_action(|_: &actions::HideApp, cx: &mut App| cx.hide());
        cx.on_action(|_: &actions::HideOthers, cx: &mut App| cx.hide_other_apps());
        cx.on_action(|_: &actions::ShowAll, cx: &mut App| cx.unhide_other_apps());
        cx.on_action(|_: &actions::NewFile, cx: &mut App| {
            if cx.windows().is_empty() {
                open_main_window(cx, Vec::new(), false);
            }
        });

        let window = open_main_window(cx, paths, !new_instance);

        // Files opened from Finder ("Open With", drag onto the Dock icon).
        let handle = window;
        cx.spawn(async move |cx| {
            while let Some(urls) = url_rx.next().await {
                let paths: Vec<PathBuf> = urls.iter().filter_map(|u| url_to_path(u)).collect();
                if paths.is_empty() {
                    continue;
                }
                let _ = cx.update(|cx| {
                    let target = handle.filter(|h| h.update(cx, |_, _, _| ()).is_ok());
                    match target {
                        Some(h) => {
                            let _ = h.update(cx, |ws, window, cx| {
                                ws.open_paths(paths, window, cx);
                                window.activate_window();
                            });
                        }
                        None => {
                            open_main_window(cx, paths, false);
                        }
                    }
                });
            }
        })
        .detach();

        // Save the session when the app is terminated by the system.
        if let Some(h) = window {
            cx.on_app_quit(move |cx| {
                let _ = h.update(cx, |ws, _, cx| {
                    if !ws.quitting {
                        ws.write_backups(cx);
                    }
                    ws.save_session(cx);
                });
                async {}
            })
            .detach();
        }
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
        if let Some(h) = window {
            debug::run_script(h, cx);
            let _ = h.update(cx, |ws, window, cx| {
                if let Some(e) = ws.active_editor() {
                    window.focus(&e.focus_handle(cx));
                }
            });
        }
    });
}
