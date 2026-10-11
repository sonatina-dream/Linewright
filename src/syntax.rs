//! Syntax highlighting on top of syntect, with incremental, checkpointed parse states so
//! that only the visible part of a document needs to be highlighted.

use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use ropey::Rope;
use syntect::highlighting::{FontStyle, HighlightIterator, HighlightState, Highlighter, Theme};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

pub const PLAIN_TEXT: &str = "Plain Text";
const CHECKPOINT_EVERY: usize = 32;
const MAX_HIGHLIGHT_LINE: usize = 20_000;

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_GENERATION: AtomicU64 = AtomicU64::new(1);

pub fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(two_face::syntax::extra_newlines)
}

pub fn bump_theme_generation() {
    THEME_GENERATION.fetch_add(1, Ordering::SeqCst);
}

fn theme_generation() -> u64 {
    THEME_GENERATION.load(Ordering::SeqCst)
}

/// Display name used in the UI for a syntax name.
pub fn display_name(syntax_name: &str) -> String {
    if syntax_name == PLAIN_TEXT {
        "Plain Text".to_string()
    } else {
        syntax_name.to_string()
    }
}

pub fn find_syntax(name: &str) -> Option<SyntaxReference> {
    if name == PLAIN_TEXT {
        return None;
    }
    syntax_set().find_syntax_by_name(name).cloned()
}

/// All visible language names, sorted case-insensitively.
pub fn language_names() -> Vec<String> {
    let mut names: Vec<String> = syntax_set()
        .syntaxes()
        .iter()
        .filter(|s| !s.hidden && !s.name.is_empty())
        .map(|s| s.name.clone())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// Pick a syntax for a file from its name, extension and first line.
pub fn detect_syntax(path: &Path, first_line: &str, _size: usize) -> String {
    let ss = syntax_set();
    let file_name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let overrides: &[(&str, &str)] = &[
        ("h", "C++"),
        ("hpp", "C++"),
        ("js", "JavaScript"),
        ("mjs", "JavaScript"),
        ("cjs", "JavaScript"),
        ("jsx", "JavaScript (Babel)"),
        ("v", "Verilog"),
        ("m", "Objective-C"),
        ("conf", "nginx"),
        ("txt", PLAIN_TEXT),
        ("log", "log"),
        ("bash", "Bourne Again Shell (bash)"),
        ("plist", "XML"),
        ("xcscheme", "XML"),
        ("storyboard", "XML"),
        ("csproj", "XML"),
        ("vcxproj", "XML"),
        ("props", "XML"),
        ("nfo", PLAIN_TEXT),
        ("reg", "INI"),
    ];
    if let Some(s) = ss.find_syntax_by_extension(&file_name) {
        return s.name.clone();
    }
    // Compound extensions like "html.erb".
    if let Some(idx) = file_name.find('.') {
        let compound = &file_name[idx + 1..];
        if compound.contains('.') {
            if let Some(s) = ss.find_syntax_by_extension(compound) {
                return s.name.clone();
            }
        }
    }
    if let Some(ext) = path.extension().map(|e| e.to_string_lossy().to_string()) {
        let lower = ext.to_lowercase();
        if let Some((_, name)) = overrides.iter().find(|(e, _)| *e == lower) {
            if *name == PLAIN_TEXT || ss.find_syntax_by_name(name).is_some() {
                return name.to_string();
            }
        }
        if let Some(s) = ss.find_syntax_by_extension(&ext).or_else(|| ss.find_syntax_by_extension(&lower)) {
            return s.name.clone();
        }
    }
    if !first_line.is_empty() {
        if let Some(s) = ss.find_syntax_by_first_line(first_line) {
            return s.name.clone();
        }
    }
    PLAIN_TEXT.to_string()
}

/// A run of identically styled characters within one line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// Length in chars.
    pub len: u32,
    /// 0xRRGGBB
    pub color: u32,
    /// Bit 0 bold, bit 1 italic, bit 2 underline.
    pub style: u8,
}

pub struct SyntaxHighlighter {
    syntax: Option<SyntaxReference>,
    checkpoints: Vec<(ParseState, HighlightState)>,
    cache: HashMap<usize, Arc<[Span]>>,
    generation: u64,
}

impl SyntaxHighlighter {
    pub fn new(syntax: Option<SyntaxReference>) -> Self {
        SyntaxHighlighter { syntax, checkpoints: Vec::new(), cache: HashMap::new(), generation: theme_generation() }
    }

    pub fn invalidate_from(&mut self, line: usize) {
        self.checkpoints.truncate(line / CHECKPOINT_EVERY + 1);
        if !self.cache.is_empty() {
            self.cache.retain(|l, _| *l < line);
        }
    }

    pub fn get(&self, line: usize) -> Option<Arc<[Span]>> {
        self.cache.get(&line).cloned()
    }

    /// Make sure spans for `lines` are cached. Returns false if the time budget ran out
    /// before everything was computed (call again on the next frame).
    pub fn ensure(&mut self, rope: &Rope, lines: Range<usize>, theme: &Theme, deadline: Instant) -> bool {
        let Some(syntax) = self.syntax.clone() else { return true };
        if self.generation != theme_generation() {
            self.checkpoints.clear();
            self.cache.clear();
            self.generation = theme_generation();
        }
        let total = rope.len_lines();
        let lines = lines.start.min(total)..lines.end.min(total);
        if lines.is_empty() || lines.clone().all(|l| self.cache.contains_key(&l)) {
            return true;
        }
        if self.cache.len() > 50_000 {
            self.cache.clear();
        }
        let ss = syntax_set();
        let highlighter = Highlighter::new(theme);
        if self.checkpoints.is_empty() {
            let ps = ParseState::new(&syntax);
            let hs = HighlightState::new(&highlighter, ScopeStack::new());
            self.checkpoints.push((ps, hs));
        }
        let target_cp = lines.start / CHECKPOINT_EVERY;
        let mut buf = String::new();
        // Advance checkpoints up to the one covering the first requested line.
        while self.checkpoints.len() <= target_cp {
            let k = self.checkpoints.len();
            let (mut ps, mut hs) = self.checkpoints[k - 1].clone();
            for line in (k - 1) * CHECKPOINT_EVERY..(k * CHECKPOINT_EVERY).min(total) {
                line_for_syntect(rope, line, &mut buf);
                if buf.len() <= MAX_HIGHLIGHT_LINE {
                    let ops = ps.parse_line(&buf, ss).unwrap_or_default();
                    for _ in HighlightIterator::new(&mut hs, &ops, &buf, &highlighter) {}
                }
            }
            self.checkpoints.push((ps, hs));
            if Instant::now() > deadline {
                return false;
            }
        }
        let (mut ps, mut hs) = self.checkpoints[target_cp].clone();
        let start = target_cp * CHECKPOINT_EVERY;
        for line in start..lines.end {
            line_for_syntect(rope, line, &mut buf);
            let want = line >= lines.start && !self.cache.contains_key(&line);
            if buf.len() > MAX_HIGHLIGHT_LINE {
                if want {
                    self.cache.insert(line, Arc::from(Vec::new()));
                }
            } else {
                let ops = ps.parse_line(&buf, ss).unwrap_or_default();
                if want {
                    let mut spans: Vec<Span> = Vec::new();
                    let content_len = buf.len() - 1; // exclude trailing '\n'
                    let mut byte_pos = 0usize;
                    for (style, piece) in HighlightIterator::new(&mut hs, &ops, &buf, &highlighter) {
                        let piece_end = byte_pos + piece.len();
                        let take_end = piece_end.min(content_len);
                        if take_end > byte_pos {
                            let n = buf[byte_pos..take_end].chars().count() as u32;
                            let color = ((style.foreground.r as u32) << 16)
                                | ((style.foreground.g as u32) << 8)
                                | style.foreground.b as u32;
                            let mut st = 0u8;
                            if style.font_style.contains(FontStyle::BOLD) {
                                st |= 1;
                            }
                            if style.font_style.contains(FontStyle::ITALIC) {
                                st |= 2;
                            }
                            if style.font_style.contains(FontStyle::UNDERLINE) {
                                st |= 4;
                            }
                            match spans.last_mut() {
                                Some(last) if last.color == color && last.style == st => last.len += n,
                                _ => spans.push(Span { len: n, color, style: st }),
                            }
                        }
                        byte_pos = piece_end;
                    }
                    self.cache.insert(line, Arc::from(spans));
                } else {
                    for _ in HighlightIterator::new(&mut hs, &ops, &buf, &highlighter) {}
                }
            }
            let next = line + 1;
            if next % CHECKPOINT_EVERY == 0 && self.checkpoints.len() == next / CHECKPOINT_EVERY {
                self.checkpoints.push((ps.clone(), hs.clone()));
            }
            if Instant::now() > deadline && line + 1 < lines.end {
                return false;
            }
        }
        true
    }
}

fn line_for_syntect(rope: &Rope, line: usize, buf: &mut String) {
    buf.clear();
    if line < rope.len_lines() {
        for chunk in rope.line(line).chunks() {
            buf.push_str(chunk);
        }
    }
    while buf.ends_with('\n') || buf.ends_with('\r') {
        buf.pop();
    }
    buf.push('\n');
}

/// Comment tokens: (line comment, block start, block end).
pub fn comment_tokens(syntax_name: &str) -> (Option<&'static str>, Option<(&'static str, &'static str)>) {
    let n = syntax_name;
    let c_like = (Some("//"), Some(("/*", "*/")));
    match n {
        "C" | "C++" | "C#" | "Java" | "JavaScript" | "JavaScript (Babel)" | "TypeScript"
        | "TypeScriptReact" | "Go" | "Rust" | "Swift" | "Kotlin" | "Scala" | "Dart" | "D"
        | "Objective-C" | "Objective-C++" | "Groovy" | "PHP" | "Less" | "SCSS" | "GLSL"
        | "Protocol Buffer" | "Solidity" | "Zig" | "Odin" | "WGSL" | "Stylus" | "QML"
        | "jsonnet" | "Typst" | "ActionScript" | "Vue Component" | "Svelte" | "JSON"
        | "GraphQL" | "Graphviz (DOT)" | "Verilog" | "SystemVerilog" | "Crystal" => {
            if n == "GraphQL" {
                (Some("#"), None)
            } else if n == "Zig" || n == "Odin" {
                (Some("//"), None)
            } else {
                c_like
            }
        }
        "CSS" => (None, Some(("/*", "*/"))),
        "Python" | "Ruby" | "Perl" | "Bourne Again Shell (bash)" | "Fish" | "Makefile"
        | "CMake" | "YAML" | "TOML" | "R" | "PowerShell" | "Dockerfile" | "Elixir" | "Nim"
        | "Julia" | "Crontab" | "Git Ignore" | "Git Attributes" | "Git Config" | "nginx"
        | "Terraform" | "Puppet" | "Requirements.txt" | "DotENV" | "Apache Conf" | "Nix"
        | "GDScript (Godot Engine)" | "Tcl" | "AWK" | "Ninja" | "Java Properties" | "Rego"
        | "Robot Framework" | "SSH Config" | "SSHD Config" | "Hosts File" | "Salt State (SLS)"
        | "Cabal" | "jq" | "JQ" | "gnuplot" | "Starlark" => match n {
            "Python" => (Some("#"), Some(("\"\"\"", "\"\"\""))),
            "PowerShell" => (Some("#"), Some(("<#", "#>"))),
            "Ruby" => (Some("#"), Some(("=begin", "=end"))),
            "Julia" => (Some("#"), Some(("#=", "=#"))),
            "Nim" => (Some("#"), Some(("#[", "]#"))),
            "Nix" => (Some("#"), Some(("/*", "*/"))),
            "Terraform" => (Some("#"), Some(("/*", "*/"))),
            _ => (Some("#"), None),
        },
        "INI" => (Some(";"), None),
        "Lisp" | "Clojure" | "Racket" => (Some(";"), Some(("#|", "|#"))),
        "x86_64 Assembly" | "ARM Assembly" | "LLVM" => (Some(";"), None),
        "SQL" | "Haskell" | "Lua" | "Ada" | "VHDL" | "Elm" | "PureScript" | "Idris" => match n {
            "Lua" => (Some("--"), Some(("--[[", "]]"))),
            "Haskell" | "Elm" | "PureScript" | "Idris" => (Some("--"), Some(("{-", "-}"))),
            "SQL" => (Some("--"), Some(("/*", "*/"))),
            _ => (Some("--"), None),
        },
        "HTML" | "XML" | "Markdown" | "HTML (Jinja2)" | "HTML (Twig)" | "HTML (Rails)"
        | "MultiMarkdown" | "Java Server Page (JSP)" | "HTML (ASP)" => (None, Some(("<!--", "-->"))),
        "Batch File" => (Some("REM "), None),
        "LaTeX" | "TeX" | "BibTeX" | "MATLAB" | "Erlang" => (Some("%"), match n {
            "MATLAB" => Some(("%{", "%}")),
            _ => None,
        }),
        "Pascal" => (Some("//"), Some(("{", "}"))),
        "OCaml" | "SML" | "F#" => match n {
            "F#" => (Some("//"), Some(("(*", "*)"))),
            _ => (None, Some(("(*", "*)"))),
        },
        "VimL" => (Some("\""), None),
        "Fortran (Modern)" => (Some("!"), None),
        "Fortran (Fixed Form)" => (Some("C "), None),
        "AppleScript" => (Some("--"), Some(("(*", "*)"))),
        "Jinja2" => (None, Some(("{#", "#}"))),
        "reStructuredText" => (Some(".. "), None),
        "Groff/troff" => (Some(".\\\" "), None),
        "Diff" | "Plain Text" => (None, None),
        _ => (Some("#"), None),
    }
}

/// Regex used by the Function List panel for a language (group 1 or "name" = function name).
pub fn function_list_pattern(syntax_name: &str) -> Option<&'static str> {
    Some(match syntax_name {
        "Rust" => r"(?m)^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+|async\s+|unsafe\s+|extern\s+\S+\s+)*(?:fn|struct|enum|trait|impl(?:<[^>]*>)?|mod|macro_rules!)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*(?:\s+for\s+[A-Za-z_][A-Za-z0-9_:<>]*)?)",
        "Python" => r"(?m)^\s*(?:async\s+)?(?:def|class)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
        "Go" => r"(?m)^func\s+(?:\([^)]*\)\s*)?(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
        "JavaScript" | "JavaScript (Babel)" | "TypeScript" | "TypeScriptReact" | "Vue Component" | "Svelte" => {
            r"(?m)^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?(?:function\*?\s+(?P<name>[A-Za-z_$][\w$]*)|class\s+(?P<cls>[A-Za-z_$][\w$]*)|(?:const|let|var)\s+(?P<arrow>[A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?(?:\([^)]*\)|[A-Za-z_$][\w$]*)\s*=>)"
        }
        "C" | "C++" | "Objective-C" | "Objective-C++" | "C#" | "Java" | "Kotlin" | "Dart" | "Scala" | "Groovy" | "D" | "Swift" => {
            match syntax_name {
                "Swift" => r"(?m)^\s*(?:@\w+\s+)*(?:(?:public|private|internal|fileprivate|open|static|final|override|mutating)\s+)*(?:func|class|struct|enum|protocol|extension)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
                "Kotlin" => r"(?m)^\s*(?:(?:public|private|internal|protected|open|override|suspend|inline|data|sealed|abstract)\s+)*(?:fun|class|object|interface)\s+(?:<[^>]*>\s*)?(?P<name>[A-Za-z_][A-Za-z0-9_.]*)",
                _ => r"(?m)^[ \t]*(?:(?:public|private|protected|internal|static|virtual|override|abstract|final|inline|extern|const|unsigned|signed|async|sealed|partial|synchronized|explicit|constexpr|friend)\s+)*(?:class|struct|interface|enum|namespace)?\s*[A-Za-z_][\w:<>,\*&\[\] ]*?[\s\*&]+(?P<name>~?[A-Za-z_][\w:]*)\s*\([^;{}]*\)\s*(?:const\s*)?(?:override\s*)?(?:noexcept\s*)?(?:throws\s+[\w., ]+)?\s*(?:\{|$)",
            }
        }
        "PHP" => r"(?m)^\s*(?:(?:public|private|protected|static|abstract|final)\s+)*(?:function|class|interface|trait)\s+(?P<name>[A-Za-z_][A-Za-z0-9_]*)",
        "Ruby" => r"(?m)^\s*(?:def|class|module)\s+(?P<name>[A-Za-z_][A-Za-z0-9_.?!=]*)",
        "Bourne Again Shell (bash)" | "Fish" => r"(?m)^\s*(?:function\s+(?P<name>[A-Za-z_][\w-]*)|(?P<fn2>[A-Za-z_][\w-]*)\s*\(\)\s*\{?)",
        "Lua" => r"(?m)^\s*(?:local\s+)?function\s+(?P<name>[A-Za-z_][\w.:]*)",
        "Perl" => r"(?m)^\s*sub\s+(?P<name>[A-Za-z_]\w*)",
        "Markdown" | "MultiMarkdown" => r"(?m)^(?P<name>#{1,6}\s+.+)$",
        "INI" | "TOML" => r"(?m)^\s*(?P<name>\[[^\]\r\n]+\])",
        "Makefile" => r"(?m)^(?P<name>[A-Za-z0-9_.\-/%$() ]+)\s*:(?:[^=]|$)",
        "SQL" => r"(?im)^\s*create\s+(?:or\s+replace\s+)?(?:procedure|function|table|view|trigger)\s+(?P<name>[\w.\[\]`]+)",
        "Elixir" => r"(?m)^\s*(?:def|defp|defmodule|defmacro)\s+(?P<name>[A-Za-z_][\w.?!]*)",
        "Haskell" => r"(?m)^(?P<name>[a-z_][\w']*)\s*::",
        "Zig" => r"(?m)^\s*(?:pub\s+)?fn\s+(?P<name>[A-Za-z_]\w*)",
        "Batch File" => r"(?m)^:(?P<name>\w+)",
        "PowerShell" => r"(?im)^\s*function\s+(?P<name>[\w-]+)",
        "Pascal" => r"(?im)^\s*(?:procedure|function|constructor|destructor)\s+(?P<name>[\w.]+)",
        "Julia" => r"(?m)^\s*function\s+(?P<name>[\w.!]+)",
        "R" => r"(?m)^\s*(?P<name>[\w.]+)\s*(?:<-|=)\s*function",
        "Nim" => r"(?m)^\s*(?:proc|func|method|template|macro|iterator)\s+(?P<name>\w+)",
        "Erlang" => r"(?m)^(?P<name>[a-z]\w*)\(.*\)\s*->",
        "Tcl" => r"(?m)^\s*proc\s+(?P<name>\S+)",
        "AppleScript" => r"(?m)^\s*on\s+(?P<name>\w+)",
        "VimL" => r"(?m)^\s*fu(?:nction)?!?\s+(?P<name>[\w:#.]+)",
        "CSS" | "SCSS" | "Less" => r"(?m)^(?P<name>[^\s{}/@][^{}\r\n]*?)\s*\{",
        "YAML" => r"(?m)^(?P<name>[A-Za-z_][\w\-]*):",
        "XML" | "HTML" => r#"(?m)<(?:\w+)[^>]*\bid\s*=\s*"(?P<name>[^"]+)""#,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_languages() {
        assert_eq!(detect_syntax(Path::new("a.rs"), "", 0), "Rust");
        assert_eq!(detect_syntax(Path::new("a.py"), "", 0), "Python");
        assert_eq!(detect_syntax(Path::new("Makefile"), "", 0), "Makefile");
        assert_eq!(detect_syntax(Path::new("a.txt"), "", 0), PLAIN_TEXT);
        assert_eq!(detect_syntax(Path::new("script"), "#!/bin/bash", 0), "Bourne Again Shell (bash)");
    }

    #[test]
    fn highlights_rust() {
        let rope = Rope::from_str("fn main() {\n    let x = 1; // hi\n}\n");
        let theme = crate::theme::load_syntect_theme("Classic");
        let mut h = SyntaxHighlighter::new(find_syntax("Rust"));
        assert!(h.ensure(&rope, 0..3, &theme, Instant::now() + std::time::Duration::from_secs(5)));
        let spans = h.get(1).unwrap();
        let total: u32 = spans.iter().map(|s| s.len).sum();
        assert_eq!(total as usize, "    let x = 1; // hi".chars().count());
        assert!(spans.len() > 2);
    }
}

/// Keywords offered by auto-completion (in addition to words from the document).
pub fn keywords(syntax_name: &str) -> &'static [&'static str] {
    const C: &[&str] = &["auto", "break", "case", "char", "const", "continue", "default", "double", "else", "enum", "extern", "float", "for", "goto", "inline", "int", "long", "register", "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned", "void", "volatile", "while", "#include", "#define", "#ifdef", "#ifndef", "#endif", "printf", "malloc", "free", "memcpy", "strlen", "NULL"];
    const CPP: &[&str] = &["alignas", "alignof", "auto", "bool", "break", "case", "catch", "char", "class", "const", "constexpr", "const_cast", "continue", "decltype", "default", "delete", "do", "double", "dynamic_cast", "else", "enum", "explicit", "export", "extern", "false", "float", "for", "friend", "goto", "if", "inline", "int", "long", "mutable", "namespace", "new", "noexcept", "nullptr", "operator", "override", "private", "protected", "public", "reinterpret_cast", "return", "short", "signed", "sizeof", "static", "static_assert", "static_cast", "struct", "switch", "template", "this", "throw", "true", "try", "typedef", "typename", "union", "unsigned", "using", "virtual", "void", "volatile", "while", "std", "string", "vector", "unique_ptr", "shared_ptr", "make_unique", "make_shared"];
    const CS: &[&str] = &["abstract", "async", "await", "base", "bool", "break", "byte", "case", "catch", "char", "class", "const", "continue", "decimal", "default", "delegate", "double", "else", "enum", "event", "explicit", "false", "finally", "float", "foreach", "interface", "internal", "namespace", "new", "null", "object", "override", "private", "protected", "public", "readonly", "record", "return", "sealed", "static", "string", "struct", "switch", "this", "throw", "true", "try", "using", "var", "virtual", "void", "while", "Console", "WriteLine"];
    const JAVA: &[&str] = &["abstract", "boolean", "break", "byte", "case", "catch", "char", "class", "continue", "default", "double", "else", "enum", "extends", "final", "finally", "float", "for", "implements", "import", "instanceof", "interface", "long", "native", "new", "null", "package", "private", "protected", "public", "return", "short", "static", "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try", "void", "volatile", "while", "String", "System", "println", "ArrayList", "HashMap", "Override"];
    const JS: &[&str] = &["async", "await", "break", "case", "catch", "class", "const", "constructor", "continue", "debugger", "default", "delete", "else", "export", "extends", "false", "finally", "for", "from", "function", "import", "instanceof", "interface", "let", "new", "null", "return", "static", "super", "switch", "this", "throw", "true", "try", "typeof", "undefined", "var", "void", "while", "yield", "console", "document", "window", "Promise", "Array", "Object", "JSON", "addEventListener", "querySelector", "setTimeout"];
    const PY: &[&str] = &["False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield", "print", "len", "range", "enumerate", "isinstance", "self", "__init__", "__name__", "__main__", "dict", "list", "tuple", "set", "str", "int", "float"];
    const RUST: &[&str] = &["as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where", "while", "Option", "Some", "None", "Result", "Ok", "Err", "String", "Vec", "Box", "println", "format", "derive", "Clone", "Debug", "Default"];
    const GO: &[&str] = &["break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough", "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range", "return", "select", "struct", "switch", "type", "var", "nil", "true", "false", "error", "string", "int", "fmt", "Println", "Printf", "make", "append", "len"];
    const PHP: &[&str] = &["abstract", "array", "as", "break", "case", "catch", "class", "const", "continue", "declare", "default", "echo", "else", "elseif", "empty", "endforeach", "endif", "extends", "final", "foreach", "function", "global", "implements", "include", "instanceof", "interface", "isset", "namespace", "new", "null", "private", "protected", "public", "require", "require_once", "return", "static", "switch", "throw", "trait", "try", "unset", "use", "while"];
    const SQL: &[&str] = &["SELECT", "FROM", "WHERE", "INSERT", "INTO", "VALUES", "UPDATE", "SET", "DELETE", "CREATE", "TABLE", "VIEW", "INDEX", "DROP", "ALTER", "JOIN", "INNER", "LEFT", "RIGHT", "OUTER", "ON", "GROUP", "ORDER", "BY", "HAVING", "LIMIT", "DISTINCT", "UNION", "AS", "AND", "OR", "NOT", "NULL", "PRIMARY", "KEY", "FOREIGN", "REFERENCES", "COUNT", "SUM", "AVG", "MIN", "MAX"];
    const HTML: &[&str] = &["html", "head", "body", "title", "meta", "link", "script", "style", "div", "span", "section", "article", "header", "footer", "nav", "main", "aside", "table", "thead", "tbody", "tr", "td", "th", "form", "input", "button", "select", "option", "textarea", "label", "img", "href", "class", "src", "alt", "type", "value", "placeholder"];
    const CSS: &[&str] = &["align-items", "background", "background-color", "border", "border-radius", "bottom", "box-shadow", "color", "cursor", "display", "flex", "flex-direction", "font-family", "font-size", "font-weight", "gap", "grid", "height", "justify-content", "left", "line-height", "margin", "max-width", "min-height", "opacity", "overflow", "padding", "position", "right", "text-align", "top", "transform", "transition", "width", "z-index", "important", "absolute", "relative", "inherit", "none"];
    const SH: &[&str] = &["if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "case", "esac", "function", "return", "local", "export", "readonly", "echo", "printf", "read", "source", "exit", "shift", "set", "unset", "test", "cd", "pwd", "grep", "sed", "awk", "find", "xargs"];
    const SWIFT: &[&str] = &["associatedtype", "class", "deinit", "enum", "extension", "fileprivate", "func", "import", "init", "inout", "internal", "let", "open", "operator", "private", "protocol", "public", "static", "struct", "subscript", "typealias", "var", "break", "case", "continue", "default", "defer", "do", "else", "fallthrough", "for", "guard", "if", "in", "repeat", "return", "switch", "where", "while", "async", "await", "throws", "try", "nil", "self", "Self", "true", "false", "print"];
    const KOTLIN: &[&str] = &["as", "break", "class", "continue", "do", "else", "false", "for", "fun", "if", "in", "interface", "is", "null", "object", "package", "return", "super", "this", "throw", "true", "try", "typealias", "val", "var", "when", "while", "data", "sealed", "override", "private", "public", "internal", "suspend", "companion", "println"];
    const LUA: &[&str] = &["and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while", "print", "pairs", "ipairs", "require", "table", "string"];
    match syntax_name {
        "C" => C,
        "C++" | "Objective-C" | "Objective-C++" => CPP,
        "C#" => CS,
        "Java" => JAVA,
        "JavaScript" | "JavaScript (Babel)" | "TypeScript" | "TypeScriptReact" | "Vue Component" | "Svelte" => JS,
        "Python" => PY,
        "Rust" => RUST,
        "Go" => GO,
        "PHP" => PHP,
        "SQL" => SQL,
        "HTML" | "XML" => HTML,
        "CSS" | "SCSS" | "Less" => CSS,
        "Bourne Again Shell (bash)" | "Fish" => SH,
        "Swift" => SWIFT,
        "Kotlin" => KOTLIN,
        "Lua" => LUA,
        _ => &[],
    }
}
