//! Field scripts (members of a/0/5/7): disassemble to labelled commands,
//! assemble back.
//!
//! A script member is:
//! - an entry table: u32 offsets, each relative to the end of its own
//!   entry, ended by the u16 0xFD13; entry n is script n of the zone;
//! - commands: u16 opcode, then the command's arguments; reached from the
//!   entries, the calls and the jumps;
//! - data reached from commands through a relative u32: movement lists and
//!   warp lists;
//! - bytes nothing reaches (dead code, padding): kept as they are.
//!
//! Relative targets are measured from the end of their own u32, as the VM
//! does (`VM_Jump(vm, vm->pc + offset)`).
//!
//! Commands are kept by name, not by opcode, so a listing assembles for
//! either family (European or Japanese opcodes).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use crate::table::{ArgKind, Family, Table};

pub const MAGIC: u16 = 0xFD13;

#[derive(Debug)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

fn err<T>(s: impl Into<String>) -> Result<T, Error> {
    Err(Error(s.into()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Num(u32),
    Label(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    Label(String),
    Cmd { name: String, args: Vec<Value> },
    Data { format: String, bytes: Vec<u8> },
    Raw(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Script {
    /// Labels of the zone's scripts, in entry order.
    pub entries: Vec<String>,
    pub items: Vec<Item>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Script,
    /// a zone's script triggers (see `zone`)
    Zone,
    Empty,
    Other,
}

fn u16_at(d: &[u8], p: usize) -> Option<u16> {
    d.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn i32_at(d: &[u8], p: usize) -> Option<i32> {
    d.get(p..p + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The entry table: absolute targets, and where the code starts.
pub fn header(d: &[u8]) -> Option<(Vec<usize>, usize)> {
    let mut entries = Vec::new();
    let mut p = 0;
    loop {
        let w = u16_at(d, p)?;
        if w == MAGIC {
            return Some((entries, p + 2));
        }
        let rel = i32_at(d, p)?;
        let t = (p as i64 + 4 + rel as i64) as usize;
        entries.push(t);
        p += 4;
    }
}

pub fn classify(d: &[u8]) -> Kind {
    if d.is_empty() {
        return Kind::Empty;
    }
    if let Some((ent, hend)) = header(d) {
        if ent.iter().all(|&e| e >= hend && e < d.len()) {
            return Kind::Script;
        }
    }
    if crate::zone::Zone::parse(d).is_some() {
        return Kind::Zone;
    }
    Kind::Other
}

/// Decoded positions (what control flow reaches).
struct Flow {
    cmds: BTreeMap<usize, (u16, Vec<(ArgKind, u32)>, usize)>,
    data: BTreeMap<usize, (&'static str, usize)>,
    targets: BTreeSet<usize>,
}

fn movement_len(d: &[u8], pos: usize) -> Option<usize> {
    let mut p = pos;
    while p + 4 <= d.len() {
        let a = u16_at(d, p)?;
        p += 4;
        if a == 0xFE {
            return Some(p - pos);
        }
    }
    None
}

fn warp_len(d: &[u8], pos: usize) -> Option<usize> {
    // rows of 5 u16, at most 8, ended by a label of 0xFFFF (its u16 counted)
    for i in 0..9 {
        let q = pos + 10 * i;
        if u16_at(d, q)? == 0xFFFF {
            return Some(10 * i + 2);
        }
    }
    None
}

fn flow(d: &[u8], t: &Table) -> Result<(Flow, usize, Vec<usize>), Error> {
    let (entries, hend) = header(d).ok_or_else(|| Error("no entry table end".into()))?;
    let mut f = Flow { cmds: BTreeMap::new(), data: BTreeMap::new(), targets: BTreeSet::new() };
    let mut work: Vec<usize> = entries.clone();
    f.targets.extend(entries.iter().copied());
    while let Some(start) = work.pop() {
        let mut pc = start;
        loop {
            if f.cmds.contains_key(&pc) {
                break;
            }
            if pc < hend || pc + 2 > d.len() {
                return err(format!("code target {pc:#x} outside the code"));
            }
            let op = u16_at(d, pc).unwrap();
            let cmd = t
                .by_op(op)
                .ok_or_else(|| Error(format!("unknown opcode {op:#x} at {pc:#x}")))?;
            let mut p = pc + 2;
            let mut args = Vec::with_capacity(cmd.args.len());
            for &k in &cmd.args {
                let n = k.size();
                let b = d.get(p..p + n).ok_or_else(|| Error(format!("arguments past the end at {pc:#x}")))?;
                let v = match n {
                    1 => b[0] as u32,
                    2 => u16::from_le_bytes([b[0], b[1]]) as u32,
                    _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
                };
                p += n;
                if k.is_target() {
                    let tgt = (p as i64 + v as i32 as i64) as usize;
                    f.targets.insert(tgt);
                    match k {
                        ArgKind::Code => work.push(tgt),
                        ArgKind::Movement | ArgKind::Warp => {
                            if !f.data.contains_key(&tgt) {
                                let (name, len) = if k == ArgKind::Movement {
                                    ("movement", movement_len(d, tgt))
                                } else {
                                    ("warp", warp_len(d, tgt))
                                };
                                let len = len.ok_or_else(|| Error(format!("{name} list at {tgt:#x} has no end")))?;
                                f.data.insert(tgt, (name, len));
                            }
                        }
                        _ => {}
                    }
                }
                args.push((k, v));
            }
            let size = p - pc;
            f.cmds.insert(pc, (op, args, size));
            if cmd.ends {
                break;
            }
            pc = p;
        }
    }
    Ok((f, hend, entries))
}

fn label_name(pos: usize) -> String {
    format!("L{pos:04x}")
}

/// Disassemble a script member. Every byte is kept: unreached bytes become
/// raw blocks, so assembling gives the member back byte for byte.
pub fn disassemble(d: &[u8], family: Family) -> Result<Script, Error> {
    let t = Table::get(family);
    let (f, hend, entries) = flow(d, t)?;
    // item starts: commands, data, targets (a target inside unreached bytes
    // splits the raw block there)
    let mut starts: BTreeSet<usize> = BTreeSet::new();
    starts.extend(f.cmds.keys().copied());
    starts.extend(f.data.keys().copied());
    starts.extend(f.targets.iter().copied());
    let mut items = Vec::new();
    let mut pos = hend;
    while pos < d.len() {
        if f.targets.contains(&pos) {
            items.push(Item::Label(label_name(pos)));
        }
        if let Some((op, args, size)) = f.cmds.get(&pos) {
            let cmd = t.by_op(*op).unwrap();
            let vals = args
                .iter()
                .enumerate()
                .map(|(i, (k, v))| {
                    if k.is_target() {
                        let end = pos + 2 + cmd.args[..=i].iter().map(|a| a.size()).sum::<usize>();
                        Value::Label(label_name((end as i64 + *v as i32 as i64) as usize))
                    } else {
                        Value::Num(*v)
                    }
                })
                .collect();
            items.push(Item::Cmd { name: cmd.name.clone(), args: vals });
            check_no_start_inside(&starts, pos, *size)?;
            pos += size;
        } else if let Some((name, len)) = f.data.get(&pos) {
            items.push(Item::Data { format: name.to_string(), bytes: d[pos..pos + len].to_vec() });
            check_no_start_inside(&starts, pos, *len)?;
            pos += len;
        } else {
            let next = starts.range(pos + 1..).next().copied().unwrap_or(d.len()).min(d.len());
            items.push(Item::Raw(d[pos..next].to_vec()));
            pos = next;
        }
    }
    for &tg in &f.targets {
        if tg >= d.len() {
            return err(format!("target {tg:#x} past the end"));
        }
    }
    Ok(Script { entries: entries.iter().map(|&e| label_name(e)).collect(), items })
}

fn check_no_start_inside(starts: &BTreeSet<usize>, pos: usize, size: usize) -> Result<(), Error> {
    if let Some(s) = starts.range(pos + 1..pos + size).next() {
        return err(format!("item at {pos:#x} covers another start at {s:#x}"));
    }
    Ok(())
}

/// Assemble a script for one family.
pub fn assemble(s: &Script, family: Family) -> Result<Vec<u8>, Error> {
    let t = Table::get(family);
    // pass 1: positions
    let mut labels: HashMap<&str, usize> = HashMap::new();
    let mut pos = 4 * s.entries.len() + 2;
    for it in &s.items {
        match it {
            Item::Label(l) => {
                if labels.insert(l.as_str(), pos).is_some() {
                    return err(format!("label {l} defined twice"));
                }
            }
            Item::Cmd { name, args } => {
                let cmd = t
                    .by_name(name)
                    .ok_or_else(|| Error(format!("no command {name} in the {} table", family.name())))?;
                if cmd.args.len() != args.len() {
                    return err(format!("{name} takes {} arguments, got {}", cmd.args.len(), args.len()));
                }
                pos += cmd.size();
            }
            Item::Data { bytes, .. } | Item::Raw(bytes) => pos += bytes.len(),
        }
    }
    let lookup = |l: &str| labels.get(l).copied().ok_or_else(|| Error(format!("label {l} not defined")));
    // pass 2: bytes
    let mut out = Vec::with_capacity(pos);
    for (i, e) in s.entries.iter().enumerate() {
        let rel = lookup(e)? as i64 - (4 * i as i64 + 4);
        out.extend_from_slice(&(rel as i32).to_le_bytes());
    }
    out.extend_from_slice(&MAGIC.to_le_bytes());
    for it in &s.items {
        match it {
            Item::Label(_) => {}
            Item::Cmd { name, args } => {
                let cmd = t.by_name(name).unwrap();
                out.extend_from_slice(&cmd.op.to_le_bytes());
                for (k, v) in cmd.args.iter().zip(args) {
                    let n = match (k.is_target(), v) {
                        (true, Value::Label(l)) => {
                            let end = out.len() + 4;
                            (lookup(l)? as i64 - end as i64) as i32 as u32
                        }
                        (false, Value::Num(n)) => *n,
                        (true, Value::Num(_)) => return err(format!("{name}: a target needs a label")),
                        (false, Value::Label(_)) => return err(format!("{name}: a number is expected")),
                    };
                    match k.size() {
                        1 => out.push(n as u8),
                        2 => out.extend_from_slice(&(n as u16).to_le_bytes()),
                        _ => out.extend_from_slice(&n.to_le_bytes()),
                    }
                }
            }
            Item::Data { bytes, .. } | Item::Raw(bytes) => out.extend_from_slice(bytes),
        }
    }
    Ok(out)
}

fn hex(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

fn unhex(s: &str) -> Result<Vec<u8>, Error> {
    if s.len() % 2 != 0 {
        return err("odd hex length");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| Error(format!("bad hex {s}"))))
        .collect()
}

/// The text form: one item per line.
///
/// ```text
/// .entries L0006 L0012
/// L0006:
///     SCRIPT_WORK_CmdLoadWorkConst 0x8020 0x0
///     SCRIPT_WORK_CmdCall @L0123
/// .data movement 0100040000fe0000
/// .raw 0000
/// ```
pub fn to_text(s: &Script) -> String {
    let mut o = String::new();
    o.push_str(".entries");
    for e in &s.entries {
        o.push(' ');
        o.push_str(e);
    }
    o.push('\n');
    for it in &s.items {
        match it {
            Item::Label(l) => {
                let _ = writeln!(o, "{l}:");
            }
            Item::Cmd { name, args } => {
                o.push_str("    ");
                o.push_str(name);
                for a in args {
                    match a {
                        Value::Num(n) => {
                            let _ = write!(o, " {n:#x}");
                        }
                        Value::Label(l) => {
                            let _ = write!(o, " @{l}");
                        }
                    }
                }
                o.push('\n');
            }
            Item::Data { format, bytes } => {
                let _ = writeln!(o, ".data {format} {}", hex(bytes));
            }
            Item::Raw(b) => {
                let _ = writeln!(o, ".raw {}", hex(b));
            }
        }
    }
    o
}

pub fn from_text(src: &str) -> Result<Script, Error> {
    let mut entries = Vec::new();
    let mut items = Vec::new();
    for (n, raw) in src.lines().enumerate() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let head = parts.next().unwrap();
        let ctx = |e: Error| Error(format!("line {}: {}", n + 1, e.0));
        if head == ".entries" {
            entries.extend(parts.map(|p| p.to_string()));
        } else if head == ".data" {
            let format = parts.next().ok_or_else(|| Error(format!("line {}: .data needs a format", n + 1)))?;
            let bytes = unhex(parts.next().unwrap_or("")).map_err(ctx)?;
            items.push(Item::Data { format: format.to_string(), bytes });
        } else if head == ".raw" {
            items.push(Item::Raw(unhex(parts.next().unwrap_or("")).map_err(ctx)?));
        } else if let Some(l) = head.strip_suffix(':') {
            items.push(Item::Label(l.to_string()));
        } else {
            let args = parts
                .map(|a| {
                    if let Some(l) = a.strip_prefix('@') {
                        Ok(Value::Label(l.to_string()))
                    } else if let Some(h) = a.strip_prefix("0x") {
                        u32::from_str_radix(h, 16).map(Value::Num).map_err(|_| Error(format!("bad number {a}")))
                    } else {
                        a.parse::<u32>().map(Value::Num).map_err(|_| Error(format!("bad number {a}")))
                    }
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(ctx)?;
            items.push(Item::Cmd { name: head.to_string(), args });
        }
    }
    Ok(Script { entries, items })
}

/// A shape of the script that does not depend on label names: every label
/// reference is replaced by the index of the item it names. Used to check
/// that a change leaves the rest of a script as it was.
pub fn shape(s: &Script) -> Vec<String> {
    let mut idx = HashMap::new();
    let mut k = 0;
    for it in &s.items {
        match it {
            Item::Label(l) => {
                idx.insert(l.clone(), k);
            }
            _ => k += 1,
        }
    }
    let name = |l: &String| idx.get(l).map(|i| format!("#{i}")).unwrap_or_else(|| format!("?{l}"));
    let mut out = vec![format!("entries {}", s.entries.iter().map(name).collect::<Vec<_>>().join(" "))];
    for it in &s.items {
        match it {
            Item::Label(_) => {}
            Item::Cmd { name: n, args } => {
                let a: Vec<String> = args
                    .iter()
                    .map(|v| match v {
                        Value::Num(x) => format!("{x:#x}"),
                        Value::Label(l) => name(l),
                    })
                    .collect();
                out.push(format!("{n} {}", a.join(" ")));
            }
            Item::Data { format, bytes } => out.push(format!(".data {format} {}", hex(bytes))),
            Item::Raw(b) => out.push(format!(".raw {}", hex(b))),
        }
    }
    out
}
