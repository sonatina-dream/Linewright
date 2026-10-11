//! Pure text transformations used by the Edit, Tools and Plugins menus.

use base64::Engine as _;
use std::time::{SystemTime, UNIX_EPOCH};

// ------------------------------------------------------------------ case conversion

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaseMode {
    Upper,
    Lower,
    Title,
    TitleKeepCaps,
    Sentence,
    SentenceKeepCaps,
    Invert,
    Random,
}

pub fn convert_case(s: &str, mode: CaseMode) -> String {
    match mode {
        CaseMode::Upper => s.to_uppercase(),
        CaseMode::Lower => s.to_lowercase(),
        CaseMode::Invert => s
            .chars()
            .flat_map(|c| {
                if c.is_uppercase() {
                    c.to_lowercase().collect::<Vec<_>>()
                } else if c.is_lowercase() {
                    c.to_uppercase().collect::<Vec<_>>()
                } else {
                    vec![c]
                }
            })
            .collect(),
        CaseMode::Random => {
            let mut rng = Rng::new();
            s.chars()
                .flat_map(|c| {
                    if rng.next() % 2 == 0 {
                        c.to_uppercase().collect::<Vec<_>>()
                    } else {
                        c.to_lowercase().collect::<Vec<_>>()
                    }
                })
                .collect()
        }
        CaseMode::Title | CaseMode::TitleKeepCaps => {
            let blend = mode == CaseMode::TitleKeepCaps;
            let mut out = String::with_capacity(s.len());
            let mut prev_alnum = false;
            for c in s.chars() {
                if c.is_alphanumeric() || c == '\'' {
                    if !prev_alnum {
                        out.extend(c.to_uppercase());
                    } else if blend {
                        out.push(c);
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    prev_alnum = c != '\'' || prev_alnum;
                } else {
                    out.push(c);
                    prev_alnum = false;
                }
            }
            out
        }
        CaseMode::Sentence | CaseMode::SentenceKeepCaps => {
            let blend = mode == CaseMode::SentenceKeepCaps;
            let mut out = String::with_capacity(s.len());
            let mut start_of_sentence = true;
            for c in s.chars() {
                if c.is_alphabetic() {
                    if start_of_sentence {
                        out.extend(c.to_uppercase());
                        start_of_sentence = false;
                    } else if blend {
                        out.push(c);
                    } else {
                        out.extend(c.to_lowercase());
                    }
                } else {
                    if c == '.' || c == '!' || c == '?' || c == '\n' || c == '\r' {
                        start_of_sentence = true;
                    } else if c.is_numeric() {
                        start_of_sentence = false;
                    }
                    out.push(c);
                }
            }
            out
        }
    }
}

// ------------------------------------------------------------------ line operations

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineOp {
    RemoveDuplicates,
    RemoveAdjacentDuplicates,
    RemoveEmpty,
    RemoveBlank,
    SortLexAsc,
    SortLexDesc,
    SortLexCiAsc,
    SortLexCiDesc,
    SortIntAsc,
    SortIntDesc,
    SortDecimalDotAsc,
    SortDecimalDotDesc,
    SortDecimalCommaAsc,
    SortDecimalCommaDesc,
    SortLengthAsc,
    SortLengthDesc,
    Reverse,
    Randomize,
    TrimTrailing,
    TrimLeading,
    TrimBoth,
}

fn leading_number(s: &str, decimal_sep: Option<char>) -> Option<f64> {
    let t = s.trim_start();
    let mut num = String::new();
    let mut seen_digit = false;
    let mut seen_sep = false;
    for (i, c) in t.chars().enumerate() {
        if i == 0 && (c == '-' || c == '+') {
            num.push(c);
        } else if c.is_ascii_digit() {
            num.push(c);
            seen_digit = true;
        } else if Some(c) == decimal_sep && !seen_sep {
            num.push('.');
            seen_sep = true;
        } else if (c == 'e' || c == 'E') && decimal_sep.is_some() && seen_digit && !num.contains('e') {
            num.push('e');
        } else {
            break;
        }
    }
    if !seen_digit {
        return None;
    }
    num.trim_end_matches(['e', '.']).parse::<f64>().ok()
}

pub fn apply_line_op(lines: Vec<String>, op: LineOp) -> Vec<String> {
    let mut lines = lines;
    match op {
        LineOp::RemoveDuplicates => {
            let mut seen = std::collections::HashSet::new();
            lines.retain(|l| seen.insert(l.clone()));
        }
        LineOp::RemoveAdjacentDuplicates => lines.dedup(),
        LineOp::RemoveEmpty => lines.retain(|l| !l.is_empty()),
        LineOp::RemoveBlank => lines.retain(|l| !l.trim().is_empty()),
        LineOp::SortLexAsc => lines.sort(),
        LineOp::SortLexDesc => {
            lines.sort();
            lines.reverse();
        }
        LineOp::SortLexCiAsc => lines.sort_by_key(|l| l.to_lowercase()),
        LineOp::SortLexCiDesc => {
            lines.sort_by_key(|l| l.to_lowercase());
            lines.reverse();
        }
        LineOp::SortIntAsc | LineOp::SortIntDesc => {
            lines.sort_by(|a, b| {
                let x = leading_number(a, None);
                let y = leading_number(b, None);
                x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
            });
            if op == LineOp::SortIntDesc {
                lines.reverse();
            }
        }
        LineOp::SortDecimalDotAsc | LineOp::SortDecimalDotDesc | LineOp::SortDecimalCommaAsc | LineOp::SortDecimalCommaDesc => {
            let sep = if matches!(op, LineOp::SortDecimalDotAsc | LineOp::SortDecimalDotDesc) { '.' } else { ',' };
            lines.sort_by(|a, b| {
                let x = leading_number(a, Some(sep));
                let y = leading_number(b, Some(sep));
                x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
            });
            if matches!(op, LineOp::SortDecimalDotDesc | LineOp::SortDecimalCommaDesc) {
                lines.reverse();
            }
        }
        LineOp::SortLengthAsc => lines.sort_by_key(|l| l.chars().count()),
        LineOp::SortLengthDesc => {
            lines.sort_by_key(|l| std::cmp::Reverse(l.chars().count()));
        }
        LineOp::Reverse => lines.reverse(),
        LineOp::Randomize => {
            let mut rng = Rng::new();
            for i in (1..lines.len()).rev() {
                let j = (rng.next() % (i as u64 + 1)) as usize;
                lines.swap(i, j);
            }
        }
        LineOp::TrimTrailing => {
            for l in lines.iter_mut() {
                *l = l.trim_end_matches([' ', '\t']).to_string();
            }
        }
        LineOp::TrimLeading => {
            for l in lines.iter_mut() {
                *l = l.trim_start_matches([' ', '\t']).to_string();
            }
        }
        LineOp::TrimBoth => {
            for l in lines.iter_mut() {
                *l = l.trim_matches([' ', '\t']).to_string();
            }
        }
    }
    lines
}

/// Replace TABs by spaces honouring tab stops.
pub fn tabs_to_spaces(line: &str, tab: usize) -> String {
    let mut out = String::new();
    let mut col = 0;
    for c in line.chars() {
        if c == '\t' {
            let n = tab - (col % tab);
            out.extend(std::iter::repeat(' ').take(n));
            col += n;
        } else {
            out.push(c);
            col += 1;
        }
    }
    out
}

/// Replace runs of spaces by TABs where they reach a tab stop.
pub fn spaces_to_tabs(line: &str, tab: usize, leading_only: bool) -> String {
    let expanded = tabs_to_spaces(line, tab);
    let chars: Vec<char> = expanded.chars().collect();
    let lead_end = if leading_only {
        chars.iter().position(|c| *c != ' ').unwrap_or(chars.len())
    } else {
        chars.len()
    };
    let mut out = String::new();
    let mut col = 0;
    let mut pending_spaces = 0;
    for (i, &c) in chars.iter().enumerate() {
        if i >= lead_end {
            out.extend(std::iter::repeat(' ').take(pending_spaces));
            out.extend(&chars[i..]);
            return out;
        }
        if c == ' ' {
            pending_spaces += 1;
            col += 1;
            if col % tab == 0 {
                if pending_spaces > 1 {
                    out.push('\t');
                } else {
                    out.push(' ');
                }
                pending_spaces = 0;
            }
        } else {
            out.extend(std::iter::repeat(' ').take(pending_spaces));
            pending_spaces = 0;
            out.push(c);
            col += 1;
        }
    }
    out.extend(std::iter::repeat(' ').take(pending_spaces));
    out
}

// ------------------------------------------------------------------ hashing / encoding

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HashKind {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl HashKind {
    pub fn label(&self) -> &'static str {
        match self {
            HashKind::Md5 => "MD5",
            HashKind::Sha1 => "SHA-1",
            HashKind::Sha256 => "SHA-256",
            HashKind::Sha512 => "SHA-512",
        }
    }
}

pub fn hash_hex(data: &[u8], kind: HashKind) -> String {
    use md5::Digest as _;
    let bytes: Vec<u8> = match kind {
        HashKind::Md5 => md5::Md5::digest(data).to_vec(),
        HashKind::Sha1 => sha1::Sha1::digest(data).to_vec(),
        HashKind::Sha256 => sha2::Sha256::digest(data).to_vec(),
        HashKind::Sha512 => sha2::Sha512::digest(data).to_vec(),
    };
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn base64_encode(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s.as_bytes())
}

pub fn base64_decode(s: &str) -> Result<String, String> {
    let cleaned: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(cleaned.trim_end_matches('=').as_bytes()))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(cleaned.trim_end_matches('=').as_bytes()))
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn url_encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC)
        .to_string()
        .replace("%2D", "-")
        .replace("%2E", ".")
        .replace("%5F", "_")
        .replace("%7E", "~")
}

pub fn url_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(&s.replace('+', " ")).decode_utf8_lossy().into_owned()
}

pub fn ascii_to_hex(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

pub fn hex_to_ascii(s: &str) -> Result<String, String> {
    let digits: String = s.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if digits.len() % 2 != 0 {
        return Err("Odd number of hex digits".into());
    }
    let bytes: Vec<u8> = (0..digits.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&digits[i..i + 2], 16).unwrap_or(0))
        .collect();
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn json_format(s: &str, indent: usize) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    let ind = " ".repeat(indent);
    let fmt = serde_json::ser::PrettyFormatter::with_indent(ind.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    serde::Serialize::serialize(&v, &mut ser).map_err(|e| e.to_string())?;
    Ok(String::from_utf8(buf).unwrap_or_default())
}

pub fn json_minify(s: &str) -> Result<String, String> {
    let v: serde_json::Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
    serde_json::to_string(&v).map_err(|e| e.to_string())
}

/// Very small XML pretty printer (Plugins > XML Tools > Pretty print).
pub fn xml_pretty(s: &str, indent: usize) -> String {
    let mut out = String::new();
    let mut depth: usize = 0;
    let mut i = 0;
    let b = s.as_bytes();
    let pad = |d: usize| " ".repeat(d * indent);
    while i < b.len() {
        if b[i] == b'<' {
            let end = match s[i..].find('>') {
                Some(e) => i + e + 1,
                None => b.len(),
            };
            let tag = &s[i..end];
            let closing = tag.starts_with("</");
            let self_closing = tag.ends_with("/>") || tag.starts_with("<?") || tag.starts_with("<!");
            if closing {
                depth = depth.saturating_sub(1);
            }
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&pad(depth));
            out.push_str(tag);
            if !closing && !self_closing {
                // Inline text content: <a>text</a> stays on one line.
                let rest = &s[end..];
                if let Some(lt) = rest.find('<') {
                    let text = &rest[..lt];
                    if rest[lt..].starts_with("</") && !text.trim().is_empty() {
                        let close_end = rest[lt..].find('>').map(|e| lt + e + 1).unwrap_or(rest.len());
                        out.push_str(text.trim());
                        out.push_str(&rest[lt..close_end]);
                        out.push('\n');
                        i = end + close_end;
                        continue;
                    }
                }
                depth += 1;
            }
            out.push('\n');
            i = end;
        } else {
            let next = s[i..].find('<').map(|e| i + e).unwrap_or(b.len());
            let text = s[i..next].trim();
            if !text.is_empty() {
                out.push_str(&pad(depth));
                out.push_str(text);
                out.push('\n');
            }
            i = next;
        }
    }
    out.trim_end().to_string()
}

// ------------------------------------------------------------------ diff (Plugins > Compare)

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffOp {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// Myers O(ND) line diff.
pub fn diff_lines(a: &[String], b: &[String]) -> Vec<DiffOp> {
    // Trim common prefix / suffix first.
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf] {
        suf += 1;
    }
    let a_mid = &a[pre..a.len() - suf];
    let b_mid = &b[pre..b.len() - suf];
    let n = a_mid.len() as isize;
    let m = b_mid.len() as isize;
    let max = (n + m) as usize;
    let mut ops = Vec::new();
    for i in 0..pre {
        ops.push(DiffOp::Equal(i, i));
    }
    if n == 0 || m == 0 || max > 400_000 {
        for i in 0..n as usize {
            ops.push(DiffOp::Delete(pre + i));
        }
        for j in 0..m as usize {
            ops.push(DiffOp::Insert(pre + j));
        }
    } else {
        let offset = max as isize;
        let mut v = vec![0isize; 2 * max + 2];
        let mut trace: Vec<Vec<isize>> = Vec::new();
        let mut found = false;
        for d in 0..=(max as isize).min(20_000) {
            trace.push(v.clone());
            let mut k = -d;
            while k <= d {
                let idx = (k + offset) as usize;
                let mut x = if k == -d || (k != d && v[idx - 1] < v[idx + 1]) { v[idx + 1] } else { v[idx - 1] + 1 };
                let mut y = x - k;
                while x < n && y < m && a_mid[x as usize] == b_mid[y as usize] {
                    x += 1;
                    y += 1;
                }
                v[idx] = x;
                if x >= n && y >= m {
                    found = true;
                    break;
                }
                k += 2;
            }
            if found {
                break;
            }
        }
        if !found {
            for i in 0..n as usize {
                ops.push(DiffOp::Delete(pre + i));
            }
            for j in 0..m as usize {
                ops.push(DiffOp::Insert(pre + j));
            }
        } else {
            // Backtrack.
            let mut x = n;
            let mut y = m;
            let mut rev = Vec::new();
            for d in (0..trace.len() as isize).rev() {
                let v = &trace[d as usize];
                let k = x - y;
                let prev_k = if k == -d || (k != d && v[(k - 1 + offset) as usize] < v[(k + 1 + offset) as usize]) { k + 1 } else { k - 1 };
                let prev_x = v[(prev_k + offset) as usize];
                let prev_y = prev_x - prev_k;
                while x > prev_x && y > prev_y {
                    rev.push(DiffOp::Equal(pre + x as usize - 1, pre + y as usize - 1));
                    x -= 1;
                    y -= 1;
                }
                if d > 0 {
                    if x == prev_x {
                        rev.push(DiffOp::Insert(pre + y as usize - 1));
                    } else {
                        rev.push(DiffOp::Delete(pre + x as usize - 1));
                    }
                }
                x = prev_x;
                y = prev_y;
            }
            rev.reverse();
            ops.extend(rev);
        }
    }
    for i in 0..suf {
        ops.push(DiffOp::Equal(a.len() - suf + i, b.len() - suf + i));
    }
    ops
}

// ------------------------------------------------------------------ misc

pub struct Rng(u64);

impl Rng {
    pub fn new() -> Self {
        let seed = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x9E3779B97F4A7C15);
        Rng(seed | 1)
    }
    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Count words as runs of word characters (View > Summary).
pub fn word_count(s: &str) -> usize {
    let mut n = 0;
    let mut in_word = false;
    for c in s.chars() {
        let w = c.is_alphanumeric() || c == '_';
        if w && !in_word {
            n += 1;
        }
        in_word = w;
    }
    n
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cases() {
        assert_eq!(convert_case("hello wORLD", CaseMode::Title), "Hello World");
        assert_eq!(convert_case("hello wORLD", CaseMode::TitleKeepCaps), "Hello WORLD");
        assert_eq!(convert_case("hello. world", CaseMode::Sentence), "Hello. World");
        assert_eq!(convert_case("AbC", CaseMode::Invert), "aBc");
    }

    #[test]
    fn line_ops() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(apply_line_op(v(&["b", "a", "b"]), LineOp::RemoveDuplicates), v(&["b", "a"]));
        assert_eq!(apply_line_op(v(&["10", "9", "-1"]), LineOp::SortIntAsc), v(&["-1", "9", "10"]));
        assert_eq!(apply_line_op(v(&["1,5", "1,25"]), LineOp::SortDecimalCommaAsc), v(&["1,25", "1,5"]));
    }

    #[test]
    fn tabs() {
        assert_eq!(tabs_to_spaces("a\tb", 4), "a   b");
        assert_eq!(spaces_to_tabs("        x", 4, true), "\t\tx");
        assert_eq!(spaces_to_tabs("a       b", 4, false), "a\t\tb");
    }

    #[test]
    fn diff() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let ops = diff_lines(&v(&["a", "b", "c", "d"]), &v(&["a", "x", "c", "d", "e"]));
        let dels: Vec<usize> = ops.iter().filter_map(|o| if let DiffOp::Delete(i) = o { Some(*i) } else { None }).collect();
        let ins: Vec<usize> = ops.iter().filter_map(|o| if let DiffOp::Insert(j) = o { Some(*j) } else { None }).collect();
        assert_eq!(dels, vec![1]);
        assert_eq!(ins, vec![1, 4]);
    }

    #[test]
    fn encodings() {
        assert_eq!(base64_decode(&base64_encode("héllo")).unwrap(), "héllo");
        assert_eq!(url_decode(&url_encode("a b&c")), "a b&c");
        assert_eq!(hex_to_ascii(&ascii_to_hex("Hi!")).unwrap(), "Hi!");
        assert_eq!(hash_hex(b"abc", HashKind::Md5), "900150983cd24fb0d6963f7d28e17f72");
    }
}
