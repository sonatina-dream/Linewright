//! Small macOS integrations that GPUI does not expose (menu check marks, window level).

use gpui::Window;

#[cfg(all(target_os = "macos", not(test)))]
mod mac {
    use std::ffi::{CStr, CString};
    use std::os::raw::c_char;

    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    use crate::menus::CHECK;

    unsafe fn walk(menu: *mut Object) {
        unsafe {
            if menu.is_null() {
                return;
            }
            let count: isize = msg_send![menu, numberOfItems];
            for i in 0..count {
                let item: *mut Object = msg_send![menu, itemAtIndex: i];
                if item.is_null() {
                    continue;
                }
                let title: *mut Object = msg_send![item, title];
                if !title.is_null() {
                    let utf8: *const c_char = msg_send![title, UTF8String];
                    if !utf8.is_null() {
                        let s = CStr::from_ptr(utf8).to_string_lossy().to_string();
                        if let Some(rest) = s.strip_prefix(CHECK) {
                            if let Ok(c) = CString::new(rest) {
                                let ns: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
                                let _: () = msg_send![item, setTitle: ns];
                                let _: () = msg_send![item, setState: 1isize];
                            }
                        }
                    }
                }
                let sub: *mut Object = msg_send![item, submenu];
                if !sub.is_null() {
                    let sub_title: *mut Object = msg_send![sub, title];
                    if !sub_title.is_null() {
                        let utf8: *const c_char = msg_send![sub_title, UTF8String];
                        if !utf8.is_null() {
                            let s = CStr::from_ptr(utf8).to_string_lossy().to_string();
                            if let Some(rest) = s.strip_prefix(CHECK) {
                                if let Ok(c) = CString::new(rest) {
                                    let ns: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
                                    let _: () = msg_send![sub, setTitle: ns];
                                    let _: () = msg_send![item, setTitle: ns];
                                    let _: () = msg_send![item, setState: 2isize];
                                }
                            }
                        }
                    }
                    walk(sub);
                }
            }
        }
    }

    pub fn apply_menu_checkmarks() {
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            if app.is_null() {
                return;
            }
            let main: *mut Object = msg_send![app, mainMenu];
            walk(main);
        }
    }

    pub fn dump_menu(menu: *mut Object, depth: usize, out: &mut String) {
        unsafe {
            if menu.is_null() || depth > 3 {
                return;
            }
            let count: isize = msg_send![menu, numberOfItems];
            for i in 0..count {
                let item: *mut Object = msg_send![menu, itemAtIndex: i];
                let sep: bool = msg_send![item, isSeparatorItem];
                if sep {
                    continue;
                }
                let title: *mut Object = msg_send![item, title];
                let utf8: *const c_char = msg_send![title, UTF8String];
                let t = if utf8.is_null() { String::new() } else { CStr::from_ptr(utf8).to_string_lossy().to_string() };
                let state: isize = msg_send![item, state];
                let key: *mut Object = msg_send![item, keyEquivalent];
                let kutf8: *const c_char = msg_send![key, UTF8String];
                let k = if kutf8.is_null() { String::new() } else { CStr::from_ptr(kutf8).to_string_lossy().to_string() };
                let mask: usize = msg_send![item, keyEquivalentModifierMask];
                let enabled: bool = msg_send![item, isEnabled];
                out.push_str(&format!(
                    "{}{}{}{}{}\n",
                    "  ".repeat(depth),
                    if state == 1 { "[x] " } else if state == 2 { "[-] " } else { "" },
                    t,
                    if k.is_empty() { String::new() } else { format!("    <{}{}>", if mask & (1 << 20) != 0 { "cmd-" } else { "" }, k) },
                    if enabled { "" } else { "  (disabled)" }
                ));
                let sub: *mut Object = msg_send![item, submenu];
                if !sub.is_null() {
                    dump_menu(sub, depth + 1, out);
                }
            }
        }
    }

    pub fn dump_main_menu() -> String {
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let main: *mut Object = msg_send![app, mainMenu];
            let mut out = String::new();
            dump_menu(main, 0, &mut out);
            out
        }
    }

    pub fn set_appearance(ns_view: *mut std::ffi::c_void, dark: bool) {
        unsafe {
            let view = ns_view as *mut Object;
            if view.is_null() {
                return;
            }
            let win: *mut Object = msg_send![view, window];
            if win.is_null() {
                return;
            }
            let name = if dark { "NSAppearanceNameDarkAqua" } else { "NSAppearanceNameAqua" };
            let c = CString::new(name).unwrap();
            let ns_name: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
            let appearance: *mut Object = msg_send![class!(NSAppearance), appearanceNamed: ns_name];
            if !appearance.is_null() {
                let _: () = msg_send![win, setAppearance: appearance];
            }
        }
    }

    pub fn set_window_level(ns_view: *mut std::ffi::c_void, floating: bool) {
        unsafe {
            let view = ns_view as *mut Object;
            if view.is_null() {
                return;
            }
            let win: *mut Object = msg_send![view, window];
            if win.is_null() {
                return;
            }
            let level: isize = if floating { 3 } else { 0 };
            let _: () = msg_send![win, setLevel: level];
        }
    }
}

/// Make the native title bar follow the editor theme (light / dark).
pub fn set_window_dark(window: &Window, dark: bool) {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(h) = HasWindowHandle::window_handle(window) {
            if let RawWindowHandle::AppKit(h) = h.as_raw() {
                mac::set_appearance(h.ns_view.as_ptr(), dark);
            }
        }
    }
    let _ = (window, dark);
}

pub fn apply_menu_checkmarks() {
    #[cfg(all(target_os = "macos", not(test)))]
    mac::apply_menu_checkmarks();
}

pub fn set_always_on_top(window: &Window, on: bool) {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(h) = HasWindowHandle::window_handle(window) {
            if let RawWindowHandle::AppKit(h) = h.as_raw() {
                mac::set_window_level(h.ns_view.as_ptr(), on);
            }
        }
    }
    let _ = (window, on);
}

/// Debug helper: textual dump of the native menu bar.
pub fn dump_main_menu() -> String {
    #[cfg(all(target_os = "macos", not(test)))]
    {
        mac::dump_main_menu()
    }
    #[cfg(not(all(target_os = "macos", not(test))))]
    {
        String::new()
    }
}
