//! Zone script triggers: the other kind of a/0/5/7 member.
//!
//! Entries, each a u16 type:
//! - 1: then an i32 offset (relative to its own end) to a list of
//!   (work u16, value u16, script u16) rows ended by a u16 0: run the
//!   script when the work variable has the value;
//! - 2, 3, 4: then a u16 script number and a u16 0 (run on map load, on
//!   recover, on the second map-load stage);
//! - 0: end of the entries.
//! After the entries come the lists. Bytes nothing points at are kept.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::script::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Cond(String),
    Script { kind: u16, script: u16, pad: u16 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tail {
    Label(String),
    /// rows of (work, value, script), the u16 0 after them implied
    Rows(Vec<(u16, u16, u16)>),
    Raw(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Zone {
    pub entries: Vec<Entry>,
    pub tail: Vec<Tail>,
}

fn u16_at(d: &[u8], p: usize) -> Option<u16> {
    d.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

impl Zone {
    pub fn parse(d: &[u8]) -> Option<Zone> {
        let mut p = 0;
        let mut entries = Vec::new();
        let mut targets = BTreeSet::new();
        loop {
            let typ = u16_at(d, p)?;
            p += 2;
            match typ {
                0 => break,
                1 => {
                    let rel = i32::from_le_bytes(d.get(p..p + 4)?.try_into().ok()?);
                    p += 4;
                    let t = (p as i64 + rel as i64) as usize;
                    targets.insert(t);
                    entries.push(Entry::Cond(format!("Z{t:04x}")));
                }
                2..=4 => {
                    let script = u16_at(d, p)?;
                    let pad = u16_at(d, p + 2)?;
                    p += 4;
                    entries.push(Entry::Script { kind: typ, script, pad });
                }
                _ => return None,
            }
        }
        let end = p;
        let mut tail = Vec::new();
        let mut pos = end;
        if targets.iter().any(|&t| t < end || t >= d.len()) {
            return None;
        }
        while pos < d.len() {
            if targets.contains(&pos) {
                tail.push(Tail::Label(format!("Z{pos:04x}")));
                let mut rows = Vec::new();
                let mut q = pos;
                loop {
                    let w = u16_at(d, q)?;
                    if w == 0 {
                        q += 2;
                        break;
                    }
                    rows.push((w, u16_at(d, q + 2)?, u16_at(d, q + 4)?));
                    q += 6;
                }
                if targets.range(pos + 1..q).next().is_some() {
                    return None;
                }
                tail.push(Tail::Rows(rows));
                pos = q;
            } else {
                let next = targets.range(pos + 1..).next().copied().unwrap_or(d.len());
                tail.push(Tail::Raw(d[pos..next].to_vec()));
                pos = next;
            }
        }
        Some(Zone { entries, tail })
    }

    pub fn assemble(&self) -> Result<Vec<u8>, Error> {
        let mut labels = std::collections::HashMap::new();
        let mut pos = self
            .entries
            .iter()
            .map(|e| match e {
                Entry::Cond(_) => 6,
                Entry::Script { .. } => 6,
            })
            .sum::<usize>()
            + 2;
        for t in &self.tail {
            match t {
                Tail::Label(l) => {
                    labels.insert(l.clone(), pos);
                }
                Tail::Rows(r) => pos += 6 * r.len() + 2,
                Tail::Raw(b) => pos += b.len(),
            }
        }
        let mut out = Vec::with_capacity(pos);
        for e in &self.entries {
            match e {
                Entry::Cond(l) => {
                    out.extend_from_slice(&1u16.to_le_bytes());
                    let t = *labels.get(l).ok_or_else(|| Error(format!("label {l} not defined")))?;
                    let rel = t as i64 - (out.len() + 4) as i64;
                    out.extend_from_slice(&(rel as i32).to_le_bytes());
                }
                Entry::Script { kind, script, pad } => {
                    out.extend_from_slice(&kind.to_le_bytes());
                    out.extend_from_slice(&script.to_le_bytes());
                    out.extend_from_slice(&pad.to_le_bytes());
                }
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
        for t in &self.tail {
            match t {
                Tail::Label(_) => {}
                Tail::Rows(r) => {
                    for (a, b, c) in r {
                        out.extend_from_slice(&a.to_le_bytes());
                        out.extend_from_slice(&b.to_le_bytes());
                        out.extend_from_slice(&c.to_le_bytes());
                    }
                    out.extend_from_slice(&0u16.to_le_bytes());
                }
                Tail::Raw(b) => out.extend_from_slice(b),
            }
        }
        Ok(out)
    }

    pub fn to_text(&self) -> String {
        let mut o = String::from(".zone\n");
        for e in &self.entries {
            match e {
                Entry::Cond(l) => {
                    let _ = writeln!(o, "    cond @{l}");
                }
                Entry::Script { kind, script, pad } => {
                    let _ = writeln!(o, "    on {kind} {script:#x} {pad:#x}");
                }
            }
        }
        for t in &self.tail {
            match t {
                Tail::Label(l) => {
                    let _ = writeln!(o, "{l}:");
                }
                Tail::Rows(r) => {
                    for (a, b, c) in r {
                        let _ = writeln!(o, "    when {a:#x} {b:#x} {c:#x}");
                    }
                    o.push_str("    end\n");
                }
                Tail::Raw(b) => {
                    o.push_str(".raw ");
                    for x in b {
                        let _ = write!(o, "{x:02x}");
                    }
                    o.push('\n');
                }
            }
        }
        o
    }

    pub fn from_text(src: &str) -> Result<Zone, Error> {
        let mut entries = Vec::new();
        let mut tail = Vec::new();
        let mut rows: Option<Vec<(u16, u16, u16)>> = None;
        let num = |s: &str| -> Result<u16, Error> {
            let r = if let Some(h) = s.strip_prefix("0x") { u16::from_str_radix(h, 16) } else { s.parse() };
            r.map_err(|_| Error(format!("bad number {s}")))
        };
        for raw in src.lines() {
            let line = raw.split(';').next().unwrap_or("").trim();
            if line.is_empty() || line == ".zone" {
                continue;
            }
            let p: Vec<&str> = line.split_whitespace().collect();
            match p[0] {
                "cond" => entries.push(Entry::Cond(p[1].trim_start_matches('@').to_string())),
                "on" => entries.push(Entry::Script { kind: num(p[1])?, script: num(p[2])?, pad: num(p[3])? }),
                "when" => rows.get_or_insert_with(Vec::new).push((num(p[1])?, num(p[2])?, num(p[3])?)),
                "end" => tail.push(Tail::Rows(rows.take().unwrap_or_default())),
                ".raw" => {
                    let h = p.get(1).copied().unwrap_or("");
                    let b = (0..h.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| Error("bad hex".into())))
                        .collect::<Result<Vec<_>, _>>()?;
                    tail.push(Tail::Raw(b));
                }
                l if l.ends_with(':') => tail.push(Tail::Label(l.trim_end_matches(':').to_string())),
                other => return Err(Error(format!("unknown zone line {other}"))),
            }
        }
        Ok(Zone { entries, tail })
    }
}
