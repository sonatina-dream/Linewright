//! Editor + UI colour themes (Settings > Themes).

use std::sync::Arc;

use gpui::{App, Global, Hsla, Rgba};
use syntect::highlighting::{Color, Theme, ThemeSet};

pub const DEFAULT_THEME: &str = "Classic";

const CLASSIC_TMTHEME: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>name</key><string>Classic</string>
<key>settings</key><array>
<dict><key>settings</key><dict>
  <key>background</key><string>#FFFFFF</string>
  <key>foreground</key><string>#000000</string>
  <key>caret</key><string>#000000</string>
  <key>lineHighlight</key><string>#E8E8FF</string>
  <key>selection</key><string>#C0C0C0</string>
  <key>gutter</key><string>#E4E4E4</string>
  <key>gutterForeground</key><string>#808080</string>
  <key>invisibles</key><string>#B4B4B4</string>
  <key>guide</key><string>#C0C0C0</string>
</dict></dict>
<dict><key>scope</key><string>comment, punctuation.definition.comment</string><key>settings</key><dict><key>foreground</key><string>#008000</string></dict></dict>
<dict><key>scope</key><string>comment.block.documentation</string><key>settings</key><dict><key>foreground</key><string>#008080</string></dict></dict>
<dict><key>scope</key><string>string, punctuation.definition.string</string><key>settings</key><dict><key>foreground</key><string>#808080</string></dict></dict>
<dict><key>scope</key><string>constant.character.escape</string><key>settings</key><dict><key>foreground</key><string>#FF8000</string></dict></dict>
<dict><key>scope</key><string>constant.numeric</string><key>settings</key><dict><key>foreground</key><string>#FF8000</string></dict></dict>
<dict><key>scope</key><string>constant.language</string><key>settings</key><dict><key>foreground</key><string>#0000FF</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>keyword, storage.modifier, keyword.control</string><key>settings</key><dict><key>foreground</key><string>#0000FF</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>storage.type, support.type, entity.name.type</string><key>settings</key><dict><key>foreground</key><string>#8000FF</string></dict></dict>
<dict><key>scope</key><string>keyword.operator, punctuation.separator, punctuation.section, punctuation.accessor</string><key>settings</key><dict><key>foreground</key><string>#000080</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>meta.preprocessor, keyword.control.import, keyword.control.directive, punctuation.definition.directive, meta.attribute, meta.annotation</string><key>settings</key><dict><key>foreground</key><string>#804000</string></dict></dict>
<dict><key>scope</key><string>entity.name.function, support.function</string><key>settings</key><dict><key>foreground</key><string>#000000</string></dict></dict>
<dict><key>scope</key><string>entity.name.tag, meta.tag punctuation.definition.tag</string><key>settings</key><dict><key>foreground</key><string>#0000FF</string></dict></dict>
<dict><key>scope</key><string>entity.other.attribute-name</string><key>settings</key><dict><key>foreground</key><string>#FF0000</string></dict></dict>
<dict><key>scope</key><string>meta.tag string, string.quoted.double.html, string.quoted.single.html, string.quoted.double.xml</string><key>settings</key><dict><key>foreground</key><string>#8000FF</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>constant.character.entity</string><key>settings</key><dict><key>foreground</key><string>#000000</string><key>fontStyle</key><string>italic</string></dict></dict>
<dict><key>scope</key><string>markup.heading, entity.name.section</string><key>settings</key><dict><key>foreground</key><string>#0000FF</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>markup.bold</string><key>settings</key><dict><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>markup.italic</string><key>settings</key><dict><key>fontStyle</key><string>italic</string></dict></dict>
<dict><key>scope</key><string>markup.underline.link, markup.underline</string><key>settings</key><dict><key>foreground</key><string>#0000FF</string><key>fontStyle</key><string>underline</string></dict></dict>
<dict><key>scope</key><string>markup.inserted</string><key>settings</key><dict><key>foreground</key><string>#008000</string></dict></dict>
<dict><key>scope</key><string>markup.deleted</string><key>settings</key><dict><key>foreground</key><string>#FF0000</string></dict></dict>
<dict><key>scope</key><string>markup.changed, meta.diff.header, meta.diff.range</string><key>settings</key><dict><key>foreground</key><string>#800080</string></dict></dict>
<dict><key>scope</key><string>variable.other.readwrite.shell, variable.other.normal, variable.other.php, variable.other.perl</string><key>settings</key><dict><key>foreground</key><string>#000080</string></dict></dict>
<dict><key>scope</key><string>meta.mapping.key string, support.type.property-name, entity.name.tag.yaml, meta.structure.dictionary.key.json string</string><key>settings</key><dict><key>foreground</key><string>#0000FF</string></dict></dict>
<dict><key>scope</key><string>entity.name.section.group-title.ini, entity.name.section.ini</string><key>settings</key><dict><key>foreground</key><string>#0000FF</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>invalid</string><key>settings</key><dict><key>foreground</key><string>#FFFFFF</string><key>background</key><string>#FF0000</string></dict></dict>
</array></dict></plist>"#;

const BASALT_TMTHEME: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>name</key><string>Basalt</string>
<key>settings</key><array>
<dict><key>settings</key><dict>
  <key>background</key><string>#22272E</string>
  <key>foreground</key><string>#D8DEE5</string>
  <key>caret</key><string>#F0C674</string>
  <key>lineHighlight</key><string>#2A3039</string>
  <key>selection</key><string>#3A4656</string>
  <key>gutter</key><string>#262C34</string>
  <key>gutterForeground</key><string>#6E7B8B</string>
  <key>guide</key><string>#38414C</string>
</dict></dict>
<dict><key>scope</key><string>comment, punctuation.definition.comment</string><key>settings</key><dict><key>foreground</key><string>#6F7D8C</string><key>fontStyle</key><string>italic</string></dict></dict>
<dict><key>scope</key><string>string, punctuation.definition.string</string><key>settings</key><dict><key>foreground</key><string>#E3B26B</string></dict></dict>
<dict><key>scope</key><string>constant.numeric</string><key>settings</key><dict><key>foreground</key><string>#E88C76</string></dict></dict>
<dict><key>scope</key><string>constant.language, constant.character.escape</string><key>settings</key><dict><key>foreground</key><string>#E88C76</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>keyword, storage.modifier, keyword.control</string><key>settings</key><dict><key>foreground</key><string>#8CC9A6</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>storage.type, support.type, entity.name.type</string><key>settings</key><dict><key>foreground</key><string>#7FB0DC</string></dict></dict>
<dict><key>scope</key><string>keyword.operator, punctuation.separator, punctuation.section, punctuation.accessor</string><key>settings</key><dict><key>foreground</key><string>#AEB8C4</string></dict></dict>
<dict><key>scope</key><string>meta.preprocessor, keyword.control.import, keyword.control.directive, meta.attribute, meta.annotation</string><key>settings</key><dict><key>foreground</key><string>#C49BE0</string></dict></dict>
<dict><key>scope</key><string>entity.name.function, support.function</string><key>settings</key><dict><key>foreground</key><string>#EEF2F6</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>entity.name.tag</string><key>settings</key><dict><key>foreground</key><string>#8CC9A6</string></dict></dict>
<dict><key>scope</key><string>entity.other.attribute-name</string><key>settings</key><dict><key>foreground</key><string>#7FB0DC</string></dict></dict>
<dict><key>scope</key><string>markup.heading, entity.name.section</string><key>settings</key><dict><key>foreground</key><string>#8CC9A6</string><key>fontStyle</key><string>bold</string></dict></dict>
<dict><key>scope</key><string>markup.inserted</string><key>settings</key><dict><key>foreground</key><string>#8CC9A6</string></dict></dict>
<dict><key>scope</key><string>markup.deleted</string><key>settings</key><dict><key>foreground</key><string>#E5757A</string></dict></dict>
<dict><key>scope</key><string>meta.mapping.key string, support.type.property-name</string><key>settings</key><dict><key>foreground</key><string>#7FB0DC</string></dict></dict>
<dict><key>scope</key><string>invalid</string><key>settings</key><dict><key>foreground</key><string>#FFFFFF</string><key>background</key><string>#B23A48</string></dict></dict>
</array></dict></plist>"#;

/// Colours for both the text area and the application chrome.
#[derive(Clone)]
#[allow(dead_code)]
pub struct AppTheme {
    pub name: String,
    pub dark: bool,
    pub syntect: Arc<Theme>,
    // text area
    pub bg: Hsla,
    pub fg: Hsla,
    pub gutter_bg: Hsla,
    pub gutter_fg: Hsla,
    pub gutter_active_fg: Hsla,
    pub margin_bg: Hsla,
    pub current_line_bg: Hsla,
    pub selection_bg: Hsla,
    pub selection_inactive_bg: Hsla,
    pub caret: Hsla,
    pub whitespace: Hsla,
    pub indent_guide: Hsla,
    pub brace_match_bg: Hsla,
    pub brace_match_fg: Hsla,
    pub brace_bad_fg: Hsla,
    pub smart_hl: Hsla,
    pub find_mark: Hsla,
    pub mark_styles: [Hsla; 5],
    pub incremental_hl: Hsla,
    pub fold_marker: Hsla,
    pub edge: Hsla,
    pub bookmark: Hsla,
    pub ctrl_bg: Hsla,
    pub ctrl_fg: Hsla,
    pub hidden_marker: Hsla,
    pub diff_added: Hsla,
    pub tag_match: Hsla,
    pub diff_removed: Hsla,
    // chrome
    pub chrome_bg: Hsla,
    pub chrome_border: Hsla,
    pub tab_bar_bg: Hsla,
    pub tab_bg: Hsla,
    pub tab_hover_bg: Hsla,
    pub tab_active_bg: Hsla,
    pub tab_accent: Hsla,
    pub tab_accent_inactive_view: Hsla,
    pub tab_fg: Hsla,
    pub tab_active_fg: Hsla,
    pub status_bg: Hsla,
    pub status_fg: Hsla,
    pub panel_bg: Hsla,
    pub panel_header_bg: Hsla,
    pub panel_fg: Hsla,
    pub muted_fg: Hsla,
    pub input_bg: Hsla,
    pub input_border: Hsla,
    pub focus_border: Hsla,
    pub button_bg: Hsla,
    pub button_hover_bg: Hsla,
    pub button_border: Hsla,
    pub accent: Hsla,
    pub accent_fg: Hsla,
    pub list_hover: Hsla,
    pub list_selected: Hsla,
    pub dialog_bg: Hsla,
    pub overlay: Hsla,
    pub unsaved: Hsla,
    pub saved: Hsla,
    pub readonly: Hsla,
    pub match_fg: Hsla,
    pub scrollbar_thumb: Hsla,
    pub scrollbar_track: Hsla,
    pub shadow: Hsla,
}

pub fn rgb_hex(c: u32) -> Hsla {
    Rgba {
        r: ((c >> 16) & 0xff) as f32 / 255.0,
        g: ((c >> 8) & 0xff) as f32 / 255.0,
        b: (c & 0xff) as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

pub fn rgba_hex(c: u32, a: f32) -> Hsla {
    let mut h = rgb_hex(c);
    h.a = a;
    h
}

fn from_syn(c: Color) -> Hsla {
    Rgba { r: c.r as f32 / 255.0, g: c.g as f32 / 255.0, b: c.b as f32 / 255.0, a: c.a as f32 / 255.0 }.into()
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let a: Rgba = a.into();
    let b: Rgba = b.into();
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
    .into()
}

fn with_alpha(mut c: Hsla, a: f32) -> Hsla {
    c.a = a;
    c
}

/// Composite a possibly translucent colour over an opaque background.
fn flatten(c: Hsla, bg: Hsla) -> Hsla {
    if c.a >= 0.999 {
        return c;
    }
    let mut o = mix(bg, with_alpha(c, 1.0), c.a);
    o.a = 1.0;
    o
}

/// Third-party themes with a known license (see `examples/export_themes.rs` and
/// `third_party/bundled-assets.md`).
fn bundled_themes() -> &'static ThemeSet {
    static SET: std::sync::OnceLock<ThemeSet> = std::sync::OnceLock::new();
    SET.get_or_init(|| syntect::dumps::from_binary(include_bytes!("../assets/themes/bundled.themedump")))
}

pub fn theme_names() -> Vec<String> {
    let mut names = vec![DEFAULT_THEME.to_string(), "Basalt".to_string()];
    let mut extra: Vec<String> = bundled_themes().themes.keys().cloned().collect();
    extra.sort_by_key(|n| n.to_lowercase());
    names.extend(extra);
    names
}

pub(crate) fn load_syntect_theme(name: &str) -> Theme {
    let parse = |src: &str| ThemeSet::load_from_reader(&mut std::io::Cursor::new(src.as_bytes())).ok();
    if name == "Basalt" {
        if let Some(t) = parse(BASALT_TMTHEME) {
            return t;
        }
    }
    if let Some(t) = bundled_themes().themes.get(name) {
        return t.clone();
    }
    parse(CLASSIC_TMTHEME).expect("built-in theme parses")
}

impl AppTheme {
    pub fn load(name: &str) -> AppTheme {
        let syn = load_syntect_theme(name);
        let s = &syn.settings;
        let bg = s.background.map(from_syn).unwrap_or(rgb_hex(0xFFFFFF));
        let bg = with_alpha(bg, 1.0);
        let classic = name == DEFAULT_THEME;
        let fg = s.foreground.map(from_syn).map(|c| flatten(c, bg)).unwrap_or(rgb_hex(0x000000));
        let dark = bg.l < 0.5;
        let sel = s.selection.map(|c| flatten(from_syn(c), bg)).unwrap_or(if dark { rgb_hex(0x264F78) } else { rgb_hex(0xC0C0C0) });
        let line_hl = s
            .line_highlight
            .map(|c| flatten(from_syn(c), bg))
            .unwrap_or(if dark { mix(bg, rgb_hex(0xFFFFFF), 0.06) } else { mix(bg, rgb_hex(0x0000FF), 0.06) });
        let gutter_bg = s
            .gutter
            .map(|c| flatten(from_syn(c), bg))
            .unwrap_or(if dark { mix(bg, rgb_hex(0xFFFFFF), 0.05) } else { mix(bg, rgb_hex(0x000000), 0.07) });
        let gutter_fg = s
            .gutter_foreground
            .map(|c| flatten(from_syn(c), gutter_bg))
            .unwrap_or(mix(fg, gutter_bg, 0.55));
        let whitespace = if classic { rgb_hex(0xB4B4B4) } else { mix(fg, bg, 0.7) };
        let guide = s.guide.map(|c| flatten(from_syn(c), bg)).unwrap_or(mix(fg, bg, 0.8));
        let caret = s.caret.map(|c| flatten(from_syn(c), bg)).unwrap_or(fg);

        let (chrome_bg, chrome_border, panel_bg, tab_bg, input_bg, status_bg) = if dark {
            (
                rgb_hex(0x2B2D30),
                rgb_hex(0x1E1F22),
                rgb_hex(0x26282B),
                rgb_hex(0x323438),
                rgb_hex(0x1E1F22),
                rgb_hex(0x2B2D30),
            )
        } else {
            (
                rgb_hex(0xF0F0F0),
                rgb_hex(0xC8C8C8),
                rgb_hex(0xF7F7F7),
                rgb_hex(0xE1E1E1),
                rgb_hex(0xFFFFFF),
                rgb_hex(0xF0F0F0),
            )
        };
        let chrome_fg = if dark { rgb_hex(0xDFE1E5) } else { rgb_hex(0x1A1A1A) };
        let accent = if dark { rgb_hex(0x3574F0) } else { rgb_hex(0x0A64D6) };

        AppTheme {
            name: name.to_string(),
            dark,
            bg,
            fg,
            gutter_bg,
            gutter_fg,
            gutter_active_fg: if classic { rgb_hex(0x000000) } else { mix(gutter_fg, fg, 0.6) },
            margin_bg: if classic { rgb_hex(0xF0F0F0) } else { gutter_bg },
            current_line_bg: line_hl,
            selection_bg: sel,
            selection_inactive_bg: mix(sel, bg, 0.3),
            caret,
            whitespace,
            indent_guide: guide,
            brace_match_bg: if dark { rgba_hex(0x5A8F3C, 0.55) } else { rgba_hex(0x88E088, 0.6) },
            brace_match_fg: if dark { rgb_hex(0xFFFFFF) } else { rgb_hex(0xFF0000) },
            brace_bad_fg: rgb_hex(0xFF0000),
            smart_hl: if dark { rgba_hex(0x3CB043, 0.35) } else { rgba_hex(0x00FF00, 0.40) },
            find_mark: if dark { rgba_hex(0xFF5555, 0.45) } else { rgba_hex(0xFF0000, 0.30) },
            mark_styles: [
                rgba_hex(0x00FFFF, if dark { 0.35 } else { 0.55 }),
                rgba_hex(0xFF8000, if dark { 0.40 } else { 0.55 }),
                rgba_hex(0xFFFF00, if dark { 0.35 } else { 0.60 }),
                rgba_hex(0x8000FF, if dark { 0.45 } else { 0.40 }),
                rgba_hex(0x008000, if dark { 0.45 } else { 0.40 }),
            ],
            incremental_hl: rgba_hex(0x0080FF, 0.35),
            fold_marker: mix(fg, bg, 0.45),
            edge: if dark { rgba_hex(0xFFFFFF, 0.15) } else { rgba_hex(0x000000, 0.15) },
            bookmark: rgb_hex(0x3A7BD5),
            ctrl_bg: if dark { rgb_hex(0xC8C8C8) } else { rgb_hex(0x000000) },
            ctrl_fg: if dark { rgb_hex(0x000000) } else { rgb_hex(0xFFFFFF) },
            hidden_marker: rgb_hex(0x1FA34A),
            tag_match: if dark { rgba_hex(0xB48EF0, 0.35) } else { rgba_hex(0x8000FF, 0.22) },
            diff_added: if dark { rgba_hex(0x2EA043, 0.28) } else { rgba_hex(0x2EA043, 0.22) },
            diff_removed: if dark { rgba_hex(0xF85149, 0.28) } else { rgba_hex(0xF85149, 0.20) },
            syntect: Arc::new(syn),
            chrome_bg,
            chrome_border,
            tab_bar_bg: chrome_bg,
            tab_bg,
            tab_hover_bg: if dark { rgb_hex(0x3A3D41) } else { rgb_hex(0xEAEAEA) },
            tab_active_bg: if dark { bg } else { rgb_hex(0xFFFFFF) },
            tab_accent: rgb_hex(0xFAAA3C),
            tab_accent_inactive_view: if dark { rgb_hex(0x7A6A55) } else { rgb_hex(0xD6C6A8) },
            tab_fg: if dark { rgb_hex(0xA6A9AE) } else { rgb_hex(0x505050) },
            tab_active_fg: chrome_fg,
            status_bg,
            status_fg: if dark { rgb_hex(0xC4C6CA) } else { rgb_hex(0x202020) },
            panel_bg,
            panel_header_bg: if dark { rgb_hex(0x313337) } else { rgb_hex(0xE8E8E8) },
            panel_fg: chrome_fg,
            muted_fg: if dark { rgb_hex(0x8C8F94) } else { rgb_hex(0x6E6E6E) },
            input_bg,
            input_border: if dark { rgb_hex(0x4A4D52) } else { rgb_hex(0xB4B4B4) },
            focus_border: accent,
            button_bg: if dark { rgb_hex(0x3A3D41) } else { rgb_hex(0xFDFDFD) },
            button_hover_bg: if dark { rgb_hex(0x45484D) } else { rgb_hex(0xE5F1FB) },
            button_border: if dark { rgb_hex(0x55585D) } else { rgb_hex(0xADADAD) },
            accent,
            accent_fg: rgb_hex(0xFFFFFF),
            list_hover: if dark { rgba_hex(0xFFFFFF, 0.06) } else { rgba_hex(0x0A64D6, 0.08) },
            list_selected: if dark { rgba_hex(0x3574F0, 0.35) } else { rgba_hex(0x0A64D6, 0.18) },
            dialog_bg: if dark { rgb_hex(0x2B2D30) } else { rgb_hex(0xF5F5F5) },
            overlay: rgba_hex(0x000000, if dark { 0.35 } else { 0.18 }),
            unsaved: rgb_hex(0xE0393E),
            saved: rgb_hex(0x2F7FD9),
            readonly: rgb_hex(0x8A8A8A),
            match_fg: if dark { rgb_hex(0xFFB86B) } else { rgb_hex(0xD00000) },
            scrollbar_thumb: if dark { rgba_hex(0xFFFFFF, 0.22) } else { rgba_hex(0x000000, 0.25) },
            scrollbar_track: if dark { rgba_hex(0xFFFFFF, 0.03) } else { rgba_hex(0x000000, 0.03) },
            shadow: rgba_hex(0x000000, if dark { 0.5 } else { 0.22 }),
        }
    }
}

pub struct ThemeState {
    pub theme: Arc<AppTheme>,
}

impl Global for ThemeState {}

pub fn init(cx: &mut App, name: &str) {
    cx.set_global(ThemeState { theme: Arc::new(AppTheme::load(name)) });
}

pub fn theme(cx: &App) -> Arc<AppTheme> {
    cx.global::<ThemeState>().theme.clone()
}

pub fn set_theme(cx: &mut App, name: &str) {
    crate::syntax::bump_theme_generation();
    cx.set_global(ThemeState { theme: Arc::new(AppTheme::load(name)) });
}
