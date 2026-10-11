//! Regenerates `assets/themes/bundled.themedump` and `third_party/bundled-assets.md`.
//!
//! Only themes with a known, redistributable license are exported. Run with:
//!     cargo run --example export_themes

use std::collections::BTreeMap;
use std::fmt::Write as _;

use syntect::highlighting::ThemeSet;
use two_face::theme::EmbeddedThemeName as T;

/// (theme, license directory inside two-face's acknowledgements, license name)
const THEMES: &[(T, &str, &str)] = &[
    (T::CatppuccinFrappe, "themes/Catppuccin", "MIT"),
    (T::CatppuccinLatte, "themes/Catppuccin", "MIT"),
    (T::CatppuccinMacchiato, "themes/Catppuccin", "MIT"),
    (T::CatppuccinMocha, "themes/Catppuccin", "MIT"),
    (T::ColdarkCold, "themes/Coldark", "MIT"),
    (T::ColdarkDark, "themes/Coldark", "MIT"),
    (T::Dracula, "themes/dracula-sublime", "MIT"),
    (T::Github, "themes/github-sublime-theme", "WTFPL"),
    (T::GruvboxDark, "themes/gruvbox", "MIT"),
    (T::GruvboxLight, "themes/gruvbox", "MIT"),
    (T::Leet, "themes/1337-Scheme", "MIT"),
    (T::MonokaiExtended, "themes/sublime-monokai-extended", "MIT"),
    (T::MonokaiExtendedBright, "themes/sublime-monokai-extended", "MIT"),
    (T::MonokaiExtendedLight, "themes/sublime-monokai-extended", "MIT"),
    (T::MonokaiExtendedOrigin, "themes/sublime-monokai-extended", "MIT"),
    (T::Nord, "themes/Nord-sublime", "MIT"),
    (T::OneHalfDark, "themes/onehalf", "MIT"),
    (T::OneHalfLight, "themes/onehalf", "MIT"),
    (T::SolarizedDark, "themes/Solarized", "MIT"),
    (T::SolarizedLight, "themes/Solarized", "MIT"),
    (T::SublimeSnazzy, "themes/sublime-snazzy", "MIT"),
    (T::TwoDark, "themes/TwoDark", "MIT"),
    (T::Zenburn, "themes/zenburn", "BSD-2-Clause"),
];

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let all = two_face::theme::extra();
    let mut set = ThemeSet { themes: BTreeMap::new() };
    for (name, _, _) in THEMES {
        set.themes.insert(name.as_name().to_string(), all.get(*name).clone());
    }
    let dump = root.join("assets/themes/bundled.themedump");
    syntect::dumps::dump_to_file(&set, &dump).expect("write theme dump");

    let ack = two_face::acknowledgement::listing();
    let mut md = String::new();
    md.push_str("# Bundled syntax definitions and color themes\n\n");
    md.push_str("Syntax definitions and color themes are provided by the two-face crate, which curates them from the bat project. ");
    md.push_str("Their licenses are reproduced below.\n\n## Syntax definitions\n\n");
    for l in ack.for_syntaxes() {
        writeln!(md, "### {}\n\n```text\n{}\n```\n", l.rel_path.display(), l.text.trim_end()).unwrap();
    }
    md.push_str("## Color themes\n\n");
    let mut done = std::collections::BTreeSet::new();
    for (_, dir, license) in THEMES {
        if !done.insert(*dir) {
            continue;
        }
        let users: Vec<&str> = THEMES.iter().filter(|(_, d, _)| d == dir).map(|(n, _, _)| n.as_name()).collect();
        writeln!(md, "### {} ({license}) — {}\n", dir.trim_start_matches("themes/"), users.join(", ")).unwrap();
        match ack.for_themes().iter().find(|l| l.rel_path.starts_with(dir)) {
            Some(l) => writeln!(md, "```text\n{}\n```\n", l.text.trim_end()).unwrap(),
            None => writeln!(md, "Licensed under the {license} license; no attribution is required.\n").unwrap(),
        }
    }
    std::fs::write(root.join("third_party/bundled-assets.md"), md).expect("write notices");
    println!("wrote {} themes to {}", set.themes.len(), dump.display());
}
