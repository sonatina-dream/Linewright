//! Text encoding detection, decoding and encoding (the "Encoding" menu).

use encoding_rs::Encoding;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextEncoding {
    /// UTF-8 without byte order mark.
    Utf8,
    /// UTF-8 with byte order mark.
    Utf8Bom,
    Utf16LeBom,
    Utf16BeBom,
    Utf16Le,
    Utf16Be,
    /// A legacy single/multi byte character set ("ANSI" and the "Character sets" submenu).
    Legacy(&'static Encoding),
}

impl Default for TextEncoding {
    fn default() -> Self {
        TextEncoding::Utf8
    }
}

/// Entries of the Encoding > Character sets submenu: (group, label, WHATWG encoding name).
pub const CHARACTER_SETS: &[(&str, &str, &str)] = &[
    ("Arabic", "ISO 8859-6", "ISO-8859-6"),
    ("Arabic", "Windows-1256", "windows-1256"),
    ("Baltic", "ISO 8859-4", "ISO-8859-4"),
    ("Baltic", "ISO 8859-13", "ISO-8859-13"),
    ("Baltic", "Windows-1257", "windows-1257"),
    ("Celtic", "ISO 8859-14", "ISO-8859-14"),
    ("Cyrillic", "ISO 8859-5", "ISO-8859-5"),
    ("Cyrillic", "KOI8-R", "KOI8-R"),
    ("Cyrillic", "KOI8-U", "KOI8-U"),
    ("Cyrillic", "Windows-1251", "windows-1251"),
    ("Cyrillic", "OEM 866", "IBM866"),
    ("Cyrillic", "Mac Cyrillic", "x-mac-cyrillic"),
    ("Central European", "ISO 8859-2", "ISO-8859-2"),
    ("Central European", "Windows-1250", "windows-1250"),
    ("Chinese", "Big5 (Traditional)", "Big5"),
    ("Chinese", "GB2312 / GBK (Simplified)", "GBK"),
    ("Chinese", "GB18030", "gb18030"),
    ("Eastern European", "ISO 8859-10", "ISO-8859-10"),
    ("Eastern European", "ISO 8859-16", "ISO-8859-16"),
    ("Greek", "ISO 8859-7", "ISO-8859-7"),
    ("Greek", "Windows-1253", "windows-1253"),
    ("Hebrew", "ISO 8859-8", "ISO-8859-8"),
    ("Hebrew", "Windows-1255", "windows-1255"),
    ("Japanese", "Shift-JIS", "Shift_JIS"),
    ("Japanese", "EUC-JP", "EUC-JP"),
    ("Japanese", "ISO-2022-JP", "ISO-2022-JP"),
    ("Korean", "EUC-KR / Windows-949", "EUC-KR"),
    ("North European", "ISO 8859-15 (Latin-9)", "ISO-8859-15"),
    ("Thai", "TIS-620 / Windows-874", "windows-874"),
    ("Turkish", "ISO 8859-3", "ISO-8859-3"),
    ("Turkish", "Windows-1254", "windows-1254"),
    ("Vietnamese", "Windows-1258", "windows-1258"),
    ("Western European", "ISO 8859-1 / Windows-1252", "windows-1252"),
    ("Western European", "Macintosh (Mac Roman)", "macintosh"),
];

impl TextEncoding {
    pub fn ansi() -> Self {
        TextEncoding::Legacy(encoding_rs::WINDOWS_1252)
    }

    /// Short label shown in the status bar.
    pub fn label(&self) -> String {
        match self {
            TextEncoding::Utf8 => "UTF-8".into(),
            TextEncoding::Utf8Bom => "UTF-8-BOM".into(),
            TextEncoding::Utf16LeBom => "UTF-16 LE BOM".into(),
            TextEncoding::Utf16BeBom => "UTF-16 BE BOM".into(),
            TextEncoding::Utf16Le => "UTF-16 LE".into(),
            TextEncoding::Utf16Be => "UTF-16 BE".into(),
            TextEncoding::Legacy(e) => {
                if *e == encoding_rs::WINDOWS_1252 {
                    "ANSI".into()
                } else {
                    e.name().to_string()
                }
            }
        }
    }

    /// Stable identifier used for persistence and action payloads.
    pub fn id(&self) -> String {
        match self {
            TextEncoding::Utf8 => "utf-8".into(),
            TextEncoding::Utf8Bom => "utf-8-bom".into(),
            TextEncoding::Utf16LeBom => "utf-16le-bom".into(),
            TextEncoding::Utf16BeBom => "utf-16be-bom".into(),
            TextEncoding::Utf16Le => "utf-16le".into(),
            TextEncoding::Utf16Be => "utf-16be".into(),
            TextEncoding::Legacy(e) => e.name().to_string(),
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "utf-8" => TextEncoding::Utf8,
            "utf-8-bom" => TextEncoding::Utf8Bom,
            "utf-16le-bom" => TextEncoding::Utf16LeBom,
            "utf-16be-bom" => TextEncoding::Utf16BeBom,
            "utf-16le" => TextEncoding::Utf16Le,
            "utf-16be" => TextEncoding::Utf16Be,
            "ansi" => TextEncoding::ansi(),
            other => TextEncoding::Legacy(Encoding::for_label(other.as_bytes())?),
        })
    }

    pub fn bom(&self) -> &'static [u8] {
        match self {
            TextEncoding::Utf8Bom => &[0xEF, 0xBB, 0xBF],
            TextEncoding::Utf16LeBom => &[0xFF, 0xFE],
            TextEncoding::Utf16BeBom => &[0xFE, 0xFF],
            _ => &[],
        }
    }

}

/// Result of decoding a file's bytes.
#[allow(dead_code)]
pub struct Decoded {
    pub text: String,
    pub encoding: TextEncoding,
    pub had_errors: bool,
}

/// Detect encoding (BOM, UTF-8 validity, UTF-16 heuristics, then a statistical guess) and decode.
pub fn detect_and_decode(bytes: &[u8]) -> Decoded {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (text, had_errors) = decode_utf8_lossy(&bytes[3..]);
        return Decoded { text, encoding: TextEncoding::Utf8Bom, had_errors };
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let text = decode_utf16(&bytes[2..], true);
        return Decoded { text, encoding: TextEncoding::Utf16LeBom, had_errors: false };
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let text = decode_utf16(&bytes[2..], false);
        return Decoded { text, encoding: TextEncoding::Utf16BeBom, had_errors: false };
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Decoded { text: s.to_string(), encoding: TextEncoding::Utf8, had_errors: false };
    }
    // BOM-less UTF-16 heuristic: lots of zero bytes on one parity.
    if bytes.len() >= 4 && bytes.len() % 2 == 0 {
        let sample = &bytes[..bytes.len().min(4096)];
        let (mut even0, mut odd0) = (0usize, 0usize);
        for (i, b) in sample.iter().enumerate() {
            if *b == 0 {
                if i % 2 == 0 { even0 += 1 } else { odd0 += 1 }
            }
        }
        let half = sample.len() / 2;
        if odd0 > half * 7 / 10 && even0 < half / 10 {
            return Decoded { text: decode_utf16(bytes, true), encoding: TextEncoding::Utf16Le, had_errors: false };
        }
        if even0 > half * 7 / 10 && odd0 < half / 10 {
            return Decoded { text: decode_utf16(bytes, false), encoding: TextEncoding::Utf16Be, had_errors: false };
        }
    }
    // If the file is mostly valid UTF-8 with a truncated tail, keep UTF-8.
    let valid_up_to = match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) => e.valid_up_to(),
    };
    if bytes.len() - valid_up_to <= 3 && valid_up_to > 0 && bytes.len() > 16 {
        let (text, had_errors) = decode_utf8_lossy(bytes);
        return Decoded { text, encoding: TextEncoding::Utf8, had_errors };
    }
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(&bytes[..bytes.len().min(1 << 20)], true);
    let enc = detector.guess(None, true);
    let enc = if enc == encoding_rs::UTF_8 { encoding_rs::WINDOWS_1252 } else { enc };
    let (text, had_errors) = enc.decode_without_bom_handling(bytes);
    Decoded { text: text.into_owned(), encoding: TextEncoding::Legacy(enc), had_errors }
}

/// Decode bytes with an explicitly chosen encoding (Encoding > "Encode in ..." / Character sets).
pub fn decode_with(bytes: &[u8], encoding: TextEncoding) -> String {
    match encoding {
        TextEncoding::Utf8 | TextEncoding::Utf8Bom => {
            let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
            decode_utf8_lossy(b).0
        }
        TextEncoding::Utf16LeBom | TextEncoding::Utf16Le => {
            let b = bytes.strip_prefix(&[0xFF, 0xFE][..]).unwrap_or(bytes);
            decode_utf16(b, true)
        }
        TextEncoding::Utf16BeBom | TextEncoding::Utf16Be => {
            let b = bytes.strip_prefix(&[0xFE, 0xFF][..]).unwrap_or(bytes);
            decode_utf16(b, false)
        }
        TextEncoding::Legacy(e) => e.decode_without_bom_handling(bytes).0.into_owned(),
    }
}

/// Encode text for writing to disk. Returns the bytes and whether some characters were unmappable.
pub fn encode(text: &str, encoding: TextEncoding) -> (Vec<u8>, bool) {
    let mut out = Vec::with_capacity(text.len() + 4);
    out.extend_from_slice(encoding.bom());
    match encoding {
        TextEncoding::Utf8 | TextEncoding::Utf8Bom => {
            out.extend_from_slice(text.as_bytes());
            (out, false)
        }
        TextEncoding::Utf16LeBom | TextEncoding::Utf16Le => {
            for u in text.encode_utf16() {
                out.extend_from_slice(&u.to_le_bytes());
            }
            (out, false)
        }
        TextEncoding::Utf16BeBom | TextEncoding::Utf16Be => {
            for u in text.encode_utf16() {
                out.extend_from_slice(&u.to_be_bytes());
            }
            (out, false)
        }
        TextEncoding::Legacy(e) => {
            let (bytes, _, unmappable) = e.encode(text);
            out.extend_from_slice(&bytes);
            (out, unmappable)
        }
    }
}

fn decode_utf8_lossy(bytes: &[u8]) -> (String, bool) {
    match String::from_utf8_lossy(bytes) {
        std::borrow::Cow::Borrowed(s) => (s.to_string(), false),
        std::borrow::Cow::Owned(s) => (s, true),
    }
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes.chunks_exact(2).map(|c| {
        if little_endian {
            u16::from_le_bytes([c[0], c[1]])
        } else {
            u16::from_be_bytes([c[0], c[1]])
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EolMode {
    Crlf,
    Lf,
    Cr,
}

impl Default for EolMode {
    fn default() -> Self {
        EolMode::Lf
    }
}

impl EolMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            EolMode::Crlf => "\r\n",
            EolMode::Lf => "\n",
            EolMode::Cr => "\r",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            EolMode::Crlf => "CRLF",
            EolMode::Lf => "LF",
            EolMode::Cr => "CR",
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            EolMode::Crlf => "crlf",
            EolMode::Lf => "lf",
            EolMode::Cr => "cr",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "crlf" => EolMode::Crlf,
            "cr" => EolMode::Cr,
            _ => EolMode::Lf,
        }
    }

    /// Detect the EOL convention from the first line break in the text.
    pub fn detect(text: &str) -> Option<EolMode> {
        let bytes = text.as_bytes();
        let idx = bytes.iter().position(|b| *b == b'\n' || *b == b'\r')?;
        if bytes[idx] == b'\n' {
            Some(EolMode::Lf)
        } else if bytes.get(idx + 1) == Some(&b'\n') {
            Some(EolMode::Crlf)
        } else {
            Some(EolMode::Cr)
        }
    }
}

/// Convert all line endings in `text` to `eol`.
pub fn normalize_eol(text: &str, eol: EolMode) -> String {
    let target = eol.as_str();
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str(target);
            }
            '\n' => out.push_str(target),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_utf16() {
        let s = "héllo wörld ✓";
        let (b, _) = encode(s, TextEncoding::Utf16LeBom);
        let d = detect_and_decode(&b);
        assert_eq!(d.encoding, TextEncoding::Utf16LeBom);
        assert_eq!(d.text, s);
    }

    #[test]
    fn detect_eol() {
        assert_eq!(EolMode::detect("a\r\nb"), Some(EolMode::Crlf));
        assert_eq!(EolMode::detect("a\nb"), Some(EolMode::Lf));
        assert_eq!(EolMode::detect("a\rb"), Some(EolMode::Cr));
        assert_eq!(normalize_eol("a\r\nb\rc\nd", EolMode::Lf), "a\nb\nc\nd");
    }

    #[test]
    fn legacy_detect() {
        let bytes = b"caf\xe9 cr\xe8me br\xfbl\xe9e and some more text here";
        let d = detect_and_decode(bytes);
        assert!(matches!(d.encoding, TextEncoding::Legacy(_)));
        assert!(d.text.contains("café"));
    }
}
