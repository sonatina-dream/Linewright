//! Development aid: `LINEWRIGHT_DEBUG_SCRIPT="sleep 800; type hello; key cmd-a; snap /tmp/a.png; quit"`
//! drives the app and saves PNG snapshots of its own window (no screen-recording permission needed).

use std::time::Duration;

use gpui::{App, AsyncApp, Keystroke, WindowHandle};

use crate::workspace::Workspace;

pub fn run_script(handle: WindowHandle<Workspace>, cx: &mut App) {
    let Ok(script) = std::env::var("LINEWRIGHT_DEBUG_SCRIPT") else { return };
    let any: gpui::AnyWindowHandle = handle.into();
    let start = std::time::Instant::now();
    cx.spawn(async move |cx: &mut AsyncApp| {
        for raw in script.split(';') {
            let cmd = raw.trim();
            if cmd.is_empty() {
                continue;
            }
            let (verb, arg) = cmd.split_once(' ').unwrap_or((cmd, ""));
            match verb {
                "sleep" => {
                    let ms = arg.trim().parse::<u64>().unwrap_or(500);
                    cx.background_executor().timer(Duration::from_millis(ms)).await;
                }
                "action" => {
                    let name = arg.trim().to_string();
                    let _ = any.update(cx, |_, window, cx| match cx.build_action(&name, None) {
                        Ok(a) => window.dispatch_action(a, cx),
                        Err(e) => eprintln!("debug: unknown action {name}: {e}"),
                    });
                }
                "key" => {
                    for k in arg.split_whitespace() {
                        let ks = Keystroke::parse(k);
                        let _ = any.update(cx, |_, window, cx| {
                            if let Ok(ks) = ks {
                                window.dispatch_keystroke(ks, cx);
                            }
                        });
                    }
                }
                "type" => {
                    let text = arg.replace("\\n", "\n").replace("\\t", "\t");
                    let _ = handle.update(cx, |ws, window, cx| {
                        if let Some(e) = ws.active_editor() {
                            e.update(cx, |e, cx| e.insert_text(&text, window, cx));
                        }
                    });
                }
                "open" => {
                    let p = std::path::PathBuf::from(arg.trim());
                    let _ = handle.update(cx, |ws, window, cx| ws.open_paths(vec![p], window, cx));
                }
                "resize" => {
                    let mut it = arg.split_whitespace().filter_map(|v| v.parse::<f32>().ok());
                    if let (Some(w), Some(h)) = (it.next(), it.next()) {
                        let _ = handle.update(cx, |_, window, _| window.resize(gpui::size(gpui::px(w), gpui::px(h))));
                    }
                }
                "snap" => {
                    let path = arg.trim().to_string();
                    let _ = handle.update(cx, |_, window, _| {
                        let ok = snapshot(window, &path);
                        eprintln!("debug: snapshot {path}: {}", if ok { "ok" } else { "FAILED" });
                    });
                }
                "ctxmenu" => {
                    let mut it = arg.split_whitespace().filter_map(|v| v.parse::<f32>().ok());
                    let (x, y) = (it.next().unwrap_or(300.0), it.next().unwrap_or(300.0));
                    let _ = handle.update(cx, |ws, _, cx| ws.show_editor_context_menu(gpui::point(gpui::px(x), gpui::px(y)), cx));
                }
                "menudump" => {
                    let path = arg.trim().to_string();
                    let _ = cx.update(|_| std::fs::write(&path, crate::platform::dump_main_menu()));
                }
                "mark" => eprintln!("debug: [{:>6} ms] {}", start.elapsed().as_millis(), arg.trim()),
                "quit" => {
                    let _ = cx.update(|cx| {
                        let _ = handle.update(cx, |ws, _, _| ws.quitting = true);
                        cx.quit()
                    });
                }
                other => eprintln!("debug: unknown command {other}"),
            }
            cx.background_executor().timer(Duration::from_millis(60)).await;
        }
    })
    .detach();
}

#[cfg(target_os = "macos")]
fn snapshot(window: &gpui::Window, path: &str) -> bool {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGSize {
        w: f64,
        h: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        o: CGPoint,
        s: CGSize,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGWindowListCreateImage(rect: CGRect, options: u32, window: u32, image_options: u32) -> *mut std::ffi::c_void;
        fn CGImageRelease(image: *mut std::ffi::c_void);
    }
    let Ok(h) = HasWindowHandle::window_handle(window) else { return false };
    let RawWindowHandle::AppKit(h) = h.as_raw() else { return false };
    unsafe {
        let view = h.ns_view.as_ptr() as *mut Object;
        let win: *mut Object = msg_send![view, window];
        if win.is_null() {
            return false;
        }
        let number: isize = msg_send![win, windowNumber];
        let null_rect = CGRect { o: CGPoint { x: f64::INFINITY, y: f64::INFINITY }, s: CGSize { w: 0.0, h: 0.0 } };
        let img = CGWindowListCreateImage(null_rect, 1 << 3, number as u32, 1 << 0);
        if img.is_null() {
            return false;
        }
        let rep: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
        let rep: *mut Object = msg_send![rep, initWithCGImage: img];
        CGImageRelease(img);
        if rep.is_null() {
            return false;
        }
        let props: *mut Object = msg_send![class!(NSDictionary), dictionary];
        // NSBitmapImageFileTypePNG = 4
        let data: *mut Object = msg_send![rep, representationUsingType: 4usize properties: props];
        if data.is_null() {
            return false;
        }
        let cpath = std::ffi::CString::new(path).unwrap_or_default();
        let ns_path: *mut Object = msg_send![class!(NSString), stringWithUTF8String: cpath.as_ptr()];
        let ok: bool = msg_send![data, writeToFile: ns_path atomically: true];
        ok
    }
}

#[cfg(not(target_os = "macos"))]
fn snapshot(_window: &gpui::Window, _path: &str) -> bool {
    false
}
