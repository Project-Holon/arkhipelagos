//! The mod's own lines (lines/*.json), rendered into the game's text units
//! the way tools/checklines.py renders and checks them.
//!
//! Syntax: `\n` (or a real line break) new line, `\r` scroll, `\c` clear,
//! `{c:N}` colour, `{buf:N}` a buffer the script fills. Names that are not
//! game text (players) are put in as plain characters.

use serde_json::Value;

const EN: &str = include_str!("../../../lines/en.json");
const FR: &str = include_str!("../../../lines/fr.json");
const DE: &str = include_str!("../../../lines/de.json");
const IT: &str = include_str!("../../../lines/it.json");
const ES: &str = include_str!("../../../lines/es.json");
const JA: &str = include_str!("../../../lines/ja.json");

/// The line `id` in the language of a game code ("IRBO", "IRAJ" ...), per
/// text block (Japanese: kana, kanji), in the line syntax.
pub fn text(code: &[u8; 4], id: &str) -> Option<Vec<String>> {
    let (src, lang) = match code[3] {
        b'F' => (FR, "fr"),
        b'D' => (DE, "de"),
        b'I' => (IT, "it"),
        b'S' => (ES, "es"),
        b'J' => (JA, "ja"),
        _ => (EN, "en"),
    };
    let v: Value = serde_json::from_str(src).ok()?;
    let found = if lang == "en" {
        v["lines"].as_array()?.iter().find(|l| l["id"] == id)?["text"].clone()
    } else {
        v["lines"][id].clone()
    };
    match found {
        Value::String(s) => Some(vec![s]),
        Value::Object(o) => Some(vec![o.get("kana")?.as_str()?.to_string(), o.get("kanji")?.as_str()?.to_string()]),
        _ => None,
    }
}

/// Line syntax -> units (no end code). `{buf:N}` is replaced by `fill(N)`
/// when it gives text, else left as the game's buffer code.
pub fn render(text: &str, fill: &dyn Fn(u32) -> Option<Vec<u16>>) -> Vec<u16> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if c == '\n' {
            out.push(0xFFFE);
            rest = &rest[1..];
        } else if let Some(r) = rest.strip_prefix("\\n") {
            out.push(0xFFFE);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("\\r") {
            out.extend([0xF000, 0xBE00, 0]);
            rest = r;
        } else if let Some(r) = rest.strip_prefix("\\c") {
            out.extend([0xF000, 0xBE01, 0]);
            rest = r;
        } else if let Some((kind, n, r)) = token(rest) {
            match kind {
                "c" => out.extend([0xF000, 0xFF00, 1, n as u16]),
                "buf" => match fill(n) {
                    Some(u) => out.extend(u),
                    None => out.extend([0xF000, 0x0100, 1, n as u16]),
                },
                _ => {}
            }
            rest = r;
        } else {
            let mut b = [0u16; 2];
            out.extend_from_slice(c.encode_utf16(&mut b));
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// `{kind:N}` at the start of `s`.
fn token(s: &str) -> Option<(&str, u32, &str)> {
    let body = s.strip_prefix('{')?;
    let end = body.find('}')?;
    let (kind, n) = body[..end].split_once(':')?;
    if !matches!(kind, "c" | "buf" | "item" | "species" | "place" | "name" | "badge") {
        return None;
    }
    Some((kind, n.parse().ok()?, &body[end + 1..]))
}

/// Plain text -> units.
pub fn plain(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Game text -> its characters only (control codes and the end dropped).
pub fn strip(units: &[u16]) -> Vec<u16> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < units.len() {
        let u = units[k];
        if u == 0xF000 && k + 2 < units.len() {
            k += 3 + units[k + 2] as usize;
            continue;
        }
        if u < 0xF000 {
            out.push(u);
        }
        k += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_the_share_lines() {
        for code in [b"IRBO", b"IRBF", b"IRBD", b"IRBI", b"IRBS", b"IRBJ"] {
            for id in ["share.received", "share.found_for", "item.other.name"] {
                let t = text(code, id).unwrap_or_else(|| panic!("{id} in {}", code[3] as char));
                assert_eq!(t.len(), if code[3] == b'J' { 2 } else { 1 });
            }
        }
    }

    #[test]
    fn renders_as_the_checker() {
        let t = text(b"IRBO", "share.received").unwrap().remove(0);
        let u = render(&t, &|n| (n == 1).then(|| plain("Misty")));
        let s: String = char::decode_utf16(u.iter().copied().filter(|&x| x < 0xF000)).map(|c| c.unwrap()).collect();
        assert!(s.starts_with("Misty sent you"), "{s}");
        assert!(u.windows(4).any(|w| w == [0xF000, 0x0100, 1, 0]), "buffer 0 kept");
        assert!(u.contains(&0xFFFE));
    }
}
