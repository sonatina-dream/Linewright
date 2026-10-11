//! Search engine: Normal / Extended / Regular expression modes, whole word, match case,
//! replacement templates and Find in Files.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::tools::is_word_char;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SearchMode {
    #[default]
    Normal,
    Extended,
    Regex,
}

#[derive(Clone, Debug)]
pub struct SearchOptions {
    pub mode: SearchMode,
    pub match_case: bool,
    pub whole_word: bool,
    pub wrap_around: bool,
    pub dot_matches_newline: bool,
    pub backward: bool,
    pub in_selection: bool,
}

impl Default for SearchOptions {
    fn default() -> Self {
        SearchOptions {
            mode: SearchMode::Normal,
            match_case: false,
            whole_word: false,
            wrap_around: true,
            dot_matches_newline: false,
            backward: false,
            in_selection: false,
        }
    }
}

enum Matcher {
    Std(regex::Regex),
    Fancy(fancy_regex::Regex),
}

pub struct SearchQuery {
    matcher: Matcher,
    whole_word: bool,
    mode: SearchMode,
    pub pattern: String,
}

/// Interpret backslash escapes of the "Extended" search mode.
pub fn unescape_extended(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let take_digits = |i: usize, n: usize, radix: u32| -> Option<(char, usize)> {
        let end = (i + n).min(chars.len());
        let digits: String = chars[i..end].iter().take_while(|c| c.is_digit(radix)).collect();
        if digits.is_empty() {
            return None;
        }
        let v = u32::from_str_radix(&digits, radix).ok()?;
        Some((char::from_u32(v)?, digits.len()))
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            i += 2;
            match n {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                '0' => out.push('\0'),
                '\\' => out.push('\\'),
                'x' => match take_digits(i, 2, 16) {
                    Some((ch, used)) => {
                        out.push(ch);
                        i += used;
                    }
                    None => out.push_str("\\x"),
                },
                'u' => match take_digits(i, 4, 16) {
                    Some((ch, used)) => {
                        out.push(ch);
                        i += used;
                    }
                    None => out.push_str("\\u"),
                },
                'o' => match take_digits(i, 3, 8) {
                    Some((ch, used)) => {
                        out.push(ch);
                        i += used;
                    }
                    None => out.push_str("\\o"),
                },
                'd' => match take_digits(i, 3, 10) {
                    Some((ch, used)) => {
                        out.push(ch);
                        i += used;
                    }
                    None => out.push_str("\\d"),
                },
                'b' => match take_digits(i, 8, 2) {
                    Some((ch, used)) => {
                        out.push(ch);
                        i += used;
                    }
                    None => out.push_str("\\b"),
                },
                other => {
                    out.push('\\');
                    out.push(other);
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

impl SearchQuery {
    pub fn new(pattern: &str, opts: &SearchOptions) -> Result<SearchQuery, String> {
        if pattern.is_empty() {
            return Err("Empty search string".into());
        }
        let source = match opts.mode {
            SearchMode::Normal => regex::escape(pattern),
            SearchMode::Extended => regex::escape(&unescape_extended(pattern)),
            SearchMode::Regex => pattern.to_string(),
        };
        let std = regex::RegexBuilder::new(&source)
            .case_insensitive(!opts.match_case)
            .multi_line(true)
            .crlf(true)
            .dot_matches_new_line(opts.dot_matches_newline && opts.mode == SearchMode::Regex)
            .size_limit(64 * 1024 * 1024)
            .build();
        let matcher = match std {
            Ok(re) => Matcher::Std(re),
            Err(std_err) => {
                let mut flags = String::from("(?m");
                if !opts.match_case {
                    flags.push('i');
                }
                if opts.dot_matches_newline {
                    flags.push('s');
                }
                flags.push(')');
                match fancy_regex::Regex::new(&format!("{flags}{source}")) {
                    Ok(re) => Matcher::Fancy(re),
                    Err(_) => return Err(format!("Invalid regular expression: {std_err}")),
                }
            }
        };
        Ok(SearchQuery { matcher, whole_word: opts.whole_word, mode: opts.mode, pattern: pattern.to_string() })
    }

    fn raw_find_at(&self, text: &str, start: usize) -> Option<Range<usize>> {
        match &self.matcher {
            Matcher::Std(re) => re.find_at(text, start).map(|m| m.range()),
            Matcher::Fancy(re) => re.find_from_pos(text, start).ok().flatten().map(|m| m.range()),
        }
    }

    fn is_whole_word(text: &str, r: &Range<usize>) -> bool {
        let before = text[..r.start].chars().next_back();
        let after = text[r.end..].chars().next();
        let first = text[r.start..r.end].chars().next();
        let last = text[r.start..r.end].chars().next_back();
        let ok_before = match (before, first) {
            (Some(b), Some(f)) => !(is_word_char(b) && is_word_char(f)),
            _ => true,
        };
        let ok_after = match (after, last) {
            (Some(a), Some(l)) => !(is_word_char(a) && is_word_char(l)),
            _ => true,
        };
        ok_before && ok_after
    }

    /// Next match starting at or after `start` and ending within `limit`.
    pub fn find_at(&self, text: &str, start: usize, limit: usize) -> Option<Range<usize>> {
        let mut pos = start;
        while pos <= limit.min(text.len()) {
            let r = self.raw_find_at(text, pos)?;
            if r.end > limit {
                return None;
            }
            if !self.whole_word || Self::is_whole_word(text, &r) {
                return Some(r);
            }
            pos = next_char_boundary(text, r.start);
        }
        None
    }

    /// All matches within `range` (byte offsets).
    pub fn find_all(&self, text: &str, range: Range<usize>, cancel: Option<&AtomicBool>) -> Vec<Range<usize>> {
        let mut out = Vec::new();
        let mut pos = range.start;
        while pos <= range.end {
            let Some(r) = self.find_at(text, pos, range.end) else { break };
            pos = if r.is_empty() { next_char_boundary(text, r.end) } else { r.end };
            out.push(r);
            if out.len() % 4096 == 0 {
                if let Some(c) = cancel {
                    if c.load(Ordering::Relaxed) {
                        break;
                    }
                }
            }
            if pos >= text.len() && out.last().map(|l| l.end >= text.len()).unwrap_or(false) {
                break;
            }
        }
        out
    }

    /// Last match that starts before `before` within `range`.
    pub fn find_prev(&self, text: &str, before: usize, range: Range<usize>) -> Option<Range<usize>> {
        let mut last = None;
        let mut pos = range.start;
        while pos <= range.end {
            let Some(r) = self.find_at(text, pos, range.end) else { break };
            if r.start >= before {
                break;
            }
            if r.end <= before {
                last = Some(r.clone());
            }
            pos = if r.is_empty() { next_char_boundary(text, r.end) } else { r.start + 1 };
            while pos < text.len() && !text.is_char_boundary(pos) {
                pos += 1;
            }
            if pos > text.len() {
                break;
            }
        }
        last
    }

    /// Expand the replacement template for the match `m`.
    pub fn expand_replacement(&self, text: &str, m: &Range<usize>, template: &str) -> String {
        match self.mode {
            SearchMode::Normal => template.to_string(),
            SearchMode::Extended => unescape_extended(template),
            SearchMode::Regex => {
                let groups = self.captures(text, m);
                expand_template(template, text, &groups.0, &groups.1)
            }
        }
    }

    fn captures(&self, text: &str, m: &Range<usize>) -> (Vec<Option<Range<usize>>>, Vec<(String, usize)>) {
        match &self.matcher {
            Matcher::Std(re) => {
                let names: Vec<(String, usize)> = re
                    .capture_names()
                    .enumerate()
                    .filter_map(|(i, n)| n.map(|n| (n.to_string(), i)))
                    .collect();
                // Re-run anchored at the match start.
                let caps = re.captures_at(text, m.start);
                let groups = match caps {
                    Some(c) if c.get(0).map(|g| g.start()) == Some(m.start) => {
                        (0..c.len()).map(|i| c.get(i).map(|g| g.range())).collect()
                    }
                    _ => vec![Some(m.clone())],
                };
                (groups, names)
            }
            Matcher::Fancy(re) => {
                let names: Vec<(String, usize)> = re
                    .capture_names()
                    .enumerate()
                    .filter_map(|(i, n)| n.map(|n| (n.to_string(), i)))
                    .collect();
                let groups = match re.captures_from_pos(text, m.start) {
                    Ok(Some(c)) if c.get(0).map(|g| g.start()) == Some(m.start) => {
                        (0..c.len()).map(|i| c.get(i).map(|g| g.range())).collect()
                    }
                    _ => vec![Some(m.clone())],
                };
                (groups, names)
            }
        }
    }
}

fn next_char_boundary(text: &str, pos: usize) -> usize {
    let mut p = pos + 1;
    while p < text.len() && !text.is_char_boundary(p) {
        p += 1;
    }
    p
}

/// Boost-like replacement template: $1 \1 ${name} $& $0 \n \r \t \\ \U \L \E \u \l.
fn expand_template(template: &str, text: &str, groups: &[Option<Range<usize>>], names: &[(String, usize)]) -> String {
    #[derive(PartialEq)]
    enum CaseState {
        None,
        Upper,
        Lower,
    }
    let mut out = String::new();
    let mut state = CaseState::None;
    let mut one_shot: Option<bool> = None; // Some(true)=upper next, Some(false)=lower next
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;
    let push = |s: &str, out: &mut String, state: &CaseState, one_shot: &mut Option<bool>| {
        for c in s.chars() {
            let c: String = if let Some(up) = one_shot.take() {
                if up { c.to_uppercase().collect() } else { c.to_lowercase().collect() }
            } else {
                match state {
                    CaseState::Upper => c.to_uppercase().collect(),
                    CaseState::Lower => c.to_lowercase().collect(),
                    CaseState::None => c.to_string(),
                }
            };
            out.push_str(&c);
        }
    };
    let group_text = |idx: usize| -> &str {
        groups.get(idx).cloned().flatten().map(|r| &text[r]).unwrap_or("")
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            i += 2;
            match n {
                'n' => push("\n", &mut out, &state, &mut one_shot),
                'r' => push("\r", &mut out, &state, &mut one_shot),
                't' => push("\t", &mut out, &state, &mut one_shot),
                '\\' => push("\\", &mut out, &state, &mut one_shot),
                'U' => state = CaseState::Upper,
                'L' => state = CaseState::Lower,
                'E' => state = CaseState::None,
                'u' => one_shot = Some(true),
                'l' => one_shot = Some(false),
                d if d.is_ascii_digit() => {
                    let g = group_text(d.to_digit(10).unwrap() as usize).to_string();
                    push(&g, &mut out, &state, &mut one_shot);
                }
                other => {
                    let s = other.to_string();
                    push(&s, &mut out, &state, &mut one_shot);
                }
            }
        } else if c == '$' && i + 1 < chars.len() {
            let n = chars[i + 1];
            if n == '&' {
                let g = group_text(0).to_string();
                push(&g, &mut out, &state, &mut one_shot);
                i += 2;
            } else if n.is_ascii_digit() {
                let mut j = i + 1;
                let mut num = 0usize;
                while j < chars.len() && chars[j].is_ascii_digit() && j < i + 3 {
                    num = num * 10 + chars[j].to_digit(10).unwrap() as usize;
                    j += 1;
                }
                let g = group_text(num).to_string();
                push(&g, &mut out, &state, &mut one_shot);
                i = j;
            } else if n == '{' {
                if let Some(close) = chars[i + 2..].iter().position(|c| *c == '}') {
                    let name: String = chars[i + 2..i + 2 + close].iter().collect();
                    let idx = name.parse::<usize>().ok().or_else(|| names.iter().find(|(n, _)| *n == name).map(|(_, i)| *i));
                    if let Some(idx) = idx {
                        let g = group_text(idx).to_string();
                        push(&g, &mut out, &state, &mut one_shot);
                    }
                    i = i + 3 + close;
                } else {
                    push("$", &mut out, &state, &mut one_shot);
                    i += 1;
                }
            } else if n == '$' {
                push("$", &mut out, &state, &mut one_shot);
                i += 2;
            } else {
                push("$", &mut out, &state, &mut one_shot);
                i += 1;
            }
        } else {
            push(&c.to_string(), &mut out, &state, &mut one_shot);
            i += 1;
        }
    }
    out
}

// ------------------------------------------------------------------ results model

#[derive(Clone, Debug)]
pub struct Hit {
    pub line: usize,
    /// The line's text (possibly truncated for display).
    pub text: String,
    /// Char ranges within `text` to highlight.
    pub ranges: Vec<Range<usize>>,
    /// Char offsets of the match in the document.
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug)]
pub struct FileHits {
    pub path: Option<PathBuf>,
    pub buffer_id: Option<u64>,
    pub name: String,
    pub hits: Vec<Hit>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ResultSet {
    pub title: String,
    pub files: Vec<FileHits>,
    pub total: usize,
    pub collapsed: bool,
}

/// Build per-line hits for matches (byte ranges) in `text`.
pub fn hits_from_matches(text: &str, matches: &[Range<usize>]) -> Vec<Hit> {
    let mut hits: Vec<Hit> = Vec::new();
    if matches.is_empty() {
        return hits;
    }
    // Walk lines incrementally.
    let bytes = text.as_bytes();
    let mut line = 0usize;
    let mut line_start = 0usize;
    let mut scan = 0usize;
    let mut char_pos_of_scan = 0usize;
    for m in matches {
        // Advance line counters up to m.start.
        while scan < m.start {
            let b = bytes[scan];
            if b == b'\n' || (b == b'\r' && bytes.get(scan + 1) != Some(&b'\n')) {
                line += 1;
                line_start = scan + 1;
            }
            if (b & 0xC0) != 0x80 {
                char_pos_of_scan += 1;
            }
            scan += 1;
        }
        let line_end = text[line_start..].find(['\n', '\r']).map(|e| line_start + e).unwrap_or(text.len());
        let match_start_char = char_pos_of_scan;
        let match_len_chars = text[m.start..m.end.min(text.len())].chars().count();
        let col_start = text[line_start..m.start].chars().count();
        let col_end = col_start + text[m.start..m.end.min(line_end).max(m.start)].chars().count();
        if let Some(last) = hits.last_mut() {
            if last.line == line {
                last.ranges.push(col_start..col_end.max(col_start));
                continue;
            }
        }
        let full_line = &text[line_start..line_end];
        let display: String = full_line.chars().take(1000).collect();
        hits.push(Hit {
            line,
            text: display,
            ranges: vec![col_start..col_end.max(col_start)],
            start: match_start_char,
            end: match_start_char + match_len_chars,
        });
    }
    hits
}

// ------------------------------------------------------------------ find in files

#[derive(Clone, Debug)]
pub struct FileSearchParams {
    pub directory: PathBuf,
    pub filters: String,
    pub recursive: bool,
    pub include_hidden: bool,
}

struct Filters {
    include: Vec<String>,
    exclude: Vec<String>,
    exclude_dirs: Vec<String>,
}

fn parse_filters(f: &str) -> Filters {
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    let mut exclude_dirs = Vec::new();
    for tok in f.split(|c: char| c == ' ' || c == ';' || c == ',').filter(|t| !t.is_empty()) {
        if let Some(rest) = tok.strip_prefix("!+\\").or_else(|| tok.strip_prefix("!+/")).or_else(|| tok.strip_prefix("!\\")).or_else(|| tok.strip_prefix("!/")) {
            exclude_dirs.push(rest.to_lowercase());
        } else if let Some(rest) = tok.strip_prefix('!') {
            exclude.push(rest.to_lowercase());
        } else {
            include.push(tok.to_lowercase());
        }
    }
    if include.is_empty() {
        include.push("*".into());
    }
    Filters { include, exclude, exclude_dirs }
}

/// Simple wildcard match supporting * and ?.
pub fn wildcard_match(pattern: &str, name: &str) -> bool {
    if pattern == "*.*" || pattern == "*" {
        return true;
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn collect_files(params: &FileSearchParams, cancel: &AtomicBool) -> Vec<PathBuf> {
    let filters = parse_filters(&params.filters);
    let mut out = Vec::new();
    let mut stack = vec![params.directory.clone()];
    while let Some(dir) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().to_string();
            if !params.include_hidden && name.starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if params.recursive && !filters.exclude_dirs.iter().any(|d| wildcard_match(d, &name.to_lowercase())) {
                    stack.push(e.path());
                }
            } else if ft.is_file() {
                let lname = name.to_lowercase();
                if filters.include.iter().any(|p| wildcard_match(p, &lname))
                    && !filters.exclude.iter().any(|p| wildcard_match(p, &lname))
                {
                    out.push(e.path());
                }
            }
        }
    }
    out.sort();
    out
}

fn looks_binary(bytes: &[u8]) -> bool {
    let n = bytes.len().min(8000);
    let sample = &bytes[..n];
    if sample.starts_with(&[0xFF, 0xFE]) || sample.starts_with(&[0xFE, 0xFF]) {
        return false;
    }
    sample.contains(&0)
}

/// Search the given files; returns per-file hits. Skips binary files and files > 256 MB.
pub fn search_in_files(files: &[PathBuf], query: &SearchQuery, cancel: &AtomicBool, progress: &dyn Fn(usize)) -> Vec<FileHits> {
    let mut out = Vec::new();
    for (i, path) in files.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if i % 50 == 0 {
            progress(i);
        }
        let Ok(meta) = std::fs::metadata(path) else { continue };
        if meta.len() > 256 * 1024 * 1024 {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else { continue };
        if looks_binary(&bytes) {
            continue;
        }
        let decoded = crate::encoding::detect_and_decode(&bytes);
        let text = decoded.text;
        let matches = query.find_all(&text, 0..text.len(), Some(cancel));
        if matches.is_empty() {
            continue;
        }
        let hits = hits_from_matches(&text, &matches);
        out.push(FileHits {
            path: Some(path.clone()),
            buffer_id: None,
            name: path.to_string_lossy().to_string(),
            hits,
        });
    }
    out
}

/// Replace in files on disk. Returns (files changed, replacements).
pub fn replace_in_files(files: &[PathBuf], query: &SearchQuery, replacement: &str, cancel: &AtomicBool) -> (usize, usize) {
    let mut files_changed = 0;
    let mut count = 0;
    for path in files {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(bytes) = std::fs::read(path) else { continue };
        if looks_binary(&bytes) {
            continue;
        }
        let decoded = crate::encoding::detect_and_decode(&bytes);
        let text = decoded.text;
        let matches = query.find_all(&text, 0..text.len(), Some(cancel));
        if matches.is_empty() {
            continue;
        }
        let new_text = replace_matches(&text, &matches, query, replacement);
        let (out, _) = crate::encoding::encode(&new_text, decoded.encoding);
        if std::fs::write(path, out).is_ok() {
            files_changed += 1;
            count += matches.len();
        }
    }
    (files_changed, count)
}

pub fn replace_matches(text: &str, matches: &[Range<usize>], query: &SearchQuery, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for m in matches {
        out.push_str(&text[last..m.start]);
        out.push_str(&query.expand_replacement(text, m, replacement));
        last = m.end;
    }
    out.push_str(&text[last..]);
    out
}


#[cfg(test)]
mod tests {
    use super::*;

    fn q(p: &str, mode: SearchMode, case: bool, word: bool) -> SearchQuery {
        SearchQuery::new(p, &SearchOptions { mode, match_case: case, whole_word: word, ..Default::default() }).unwrap()
    }

    #[test]
    fn normal_search() {
        let t = "Foo foo food";
        let s = q("foo", SearchMode::Normal, false, false);
        assert_eq!(s.find_all(t, 0..t.len(), None).len(), 3);
        let s = q("foo", SearchMode::Normal, true, true);
        assert_eq!(s.find_all(t, 0..t.len(), None), vec![4..7]);
        let s = q("a.b", SearchMode::Normal, true, false);
        assert!(s.find_all("axb", 0..3, None).is_empty());
    }

    #[test]
    fn extended_and_regex() {
        let t = "a\tb\r\nc";
        let s = q("\\t", SearchMode::Extended, true, false);
        assert_eq!(s.find_all(t, 0..t.len(), None), vec![1..2]);
        let s = q("b$", SearchMode::Regex, true, false);
        assert_eq!(s.find_all(t, 0..t.len(), None), vec![2..3]);
        let s = q("(\\w)\\1", SearchMode::Regex, true, false);
        assert_eq!(s.find_all("abbc", 0..4, None), vec![1..3]);
    }

    #[test]
    fn replacement_templates() {
        let t = "john smith";
        let s = q("(\\w+) (\\w+)", SearchMode::Regex, true, false);
        let m = s.find_at(t, 0, t.len()).unwrap();
        assert_eq!(s.expand_replacement(t, &m, "$2, \\1"), "smith, john");
        assert_eq!(s.expand_replacement(t, &m, "\\U$1\\E-\\u$2"), "JOHN-Smith");
    }

    #[test]
    fn find_prev_works() {
        let t = "ab ab ab";
        let s = q("ab", SearchMode::Normal, true, false);
        assert_eq!(s.find_prev(t, 6, 0..t.len()), Some(3..5));
        assert_eq!(s.find_prev(t, 1, 0..t.len()), None);
    }

    #[test]
    fn wildcards() {
        assert!(wildcard_match("*.rs", "main.rs"));
        assert!(!wildcard_match("*.rs", "main.rsx"));
        assert!(wildcard_match("ma?n.*", "main.c"));
    }

    #[test]
    fn hits() {
        let t = "one\ntwo two\nthree";
        let s = q("two", SearchMode::Normal, true, false);
        let m = s.find_all(t, 0..t.len(), None);
        let h = hits_from_matches(t, &m);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].line, 1);
        assert_eq!(h[0].ranges, vec![0..3, 4..7]);
        assert_eq!(h[0].start, 4);
    }
}
