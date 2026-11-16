//! Edits that never move existing bytes.
//!
//! - `Detour`: the commands at a place (at least 6 bytes, no label inside)
//!   become a `Jump` to new code appended at the end, padded with `Nop`;
//!   the new code may jump back to a resume place (a label is added there).
//! - `Replace`: one command becomes another of the same size.
//!
//! Places are command ordinals: the index of a command among the script's
//! commands as `disassemble` lists them (labels, data and raw bytes are not
//! counted). All places refer to the script before any edit.

use crate::script::{Error, Item, Script, Value};
use crate::table::{Family, Table};

/// The label a detour body uses to jump back to its resume place.
pub const RESUME: &str = "@resume";

pub struct Detour {
    /// first command to cover
    pub at: usize,
    /// how many commands to cover
    pub cover: usize,
    /// the new code; `Value::Label(RESUME)` names the resume place
    pub body: Vec<Item>,
    /// where the body goes back to, if it does
    pub resume: Option<usize>,
}

pub struct Replace {
    pub at: usize,
    pub with: Item,
}

/// A command item.
pub fn cmd(name: &str, args: &[u32]) -> Item {
    Item::Cmd { name: name.to_string(), args: args.iter().map(|&a| Value::Num(a)).collect() }
}

/// A command whose one argument is a label.
pub fn cmd_label(name: &str, label: &str) -> Item {
    Item::Cmd { name: name.to_string(), args: vec![Value::Label(label.to_string())] }
}

fn err<T>(s: impl Into<String>) -> Result<T, Error> {
    Err(Error(s.into()))
}

/// Item index of each command ordinal.
pub fn cmd_items(s: &Script) -> Vec<usize> {
    s.items.iter().enumerate().filter(|(_, it)| matches!(it, Item::Cmd { .. })).map(|(i, _)| i).collect()
}

fn size_of(t: &Table, it: &Item) -> Result<usize, Error> {
    match it {
        Item::Cmd { name, .. } => Ok(t.by_name(name).ok_or_else(|| Error(format!("no command {name}")))?.size()),
        Item::Data { bytes, .. } | Item::Raw(bytes) => Ok(bytes.len()),
        Item::Label(_) => Ok(0),
    }
}

/// The command at an ordinal (for checks before an edit).
pub fn cmd_at(s: &Script, ordinal: usize) -> Option<(&str, Vec<u32>)> {
    let idx = *cmd_items(s).get(ordinal)?;
    match &s.items[idx] {
        Item::Cmd { name, args } => Some((
            name.as_str(),
            args.iter().map(|a| if let Value::Num(n) = a { *n } else { u32::MAX }).collect(),
        )),
        _ => None,
    }
}

/// Whether `cover` commands from ordinal `at` can hold a detour: no label
/// after the first, only commands, and room for a jump plus whole Nops.
pub fn coverable(s: &Script, family: Family, at: usize, cover: usize) -> bool {
    let t = Table::get(family);
    let ci = cmd_items(s);
    let (Some(&first), Some(&last)) = (ci.get(at), ci.get(at + cover.max(1) - 1)) else { return false };
    let mut size = 0;
    for i in first..=last {
        match &s.items[i] {
            Item::Label(_) => return false,
            it @ Item::Cmd { .. } => size += size_of(t, it).unwrap_or(usize::MAX / 4),
            _ => return false,
        }
    }
    let jump = t.by_name("SCRIPT_WORK_CmdJump").map(|c| c.size()).unwrap_or(6);
    let nop = t.by_name("SCRIPT_WORK_CmdNop").map(|c| c.size()).unwrap_or(2);
    size >= jump && (size - jump) % nop == 0
}

/// Byte offset and size of each command, by ordinal, in the assembled
/// member (the entry table and its 0xFD13 first).
pub fn cmd_offsets(s: &Script, family: Family) -> Result<Vec<(usize, usize)>, Error> {
    let t = Table::get(family);
    let mut pos = 4 * s.entries.len() + 2;
    let mut out = Vec::new();
    for it in &s.items {
        let n = size_of(t, it)?;
        if matches!(it, Item::Cmd { .. }) {
            out.push((pos, n));
        }
        pos += n;
    }
    Ok(out)
}

/// Apply detours and replacements to a script. Bodies are appended at the
/// end in the order given. Returns the ordinals of the commands changed.
pub fn apply(s: &mut Script, family: Family, detours: Vec<Detour>, replaces: Vec<Replace>) -> Result<Vec<usize>, Error> {
    let t = Table::get(family);
    let ci = cmd_items(s);
    let ord_of: std::collections::BTreeMap<usize, usize> = ci.iter().enumerate().map(|(o, &i)| (i, o)).collect();
    let item_of = |o: usize| ci.get(o).copied().ok_or_else(|| Error(format!("no command {o}")));
    // what changes at which item index: (index, new items, labels to put before)
    let mut at_index: Vec<(usize, Vec<Item>)> = Vec::new();
    let mut resume_labels: Vec<(usize, String)> = Vec::new();
    let mut tail: Vec<Item> = Vec::new();
    let mut used: std::collections::BTreeSet<usize> = Default::default();
    for (k, d) in detours.into_iter().enumerate() {
        if d.cover == 0 {
            return err("a detour covers no command");
        }
        let first = item_of(d.at)?;
        let last = item_of(d.at + d.cover - 1)?;
        let mut size = 0;
        for i in first..=last {
            if i > first && matches!(s.items[i], Item::Label(_)) {
                return err(format!("command {}: a label inside the covered commands", d.at));
            }
            if !matches!(s.items[i], Item::Cmd { .. } | Item::Label(_)) {
                return err(format!("command {}: data inside the covered commands", d.at));
            }
            if !used.insert(i) {
                return err(format!("command {}: covered twice", d.at));
            }
            size += size_of(t, &s.items[i])?;
        }
        let jump = t.by_name("SCRIPT_WORK_CmdJump").ok_or_else(|| Error("no Jump".into()))?.size();
        let nop = t.by_name("SCRIPT_WORK_CmdNop").ok_or_else(|| Error("no Nop".into()))?.size();
        if size < jump || (size - jump) % nop != 0 {
            return err(format!("command {}: {size} bytes cannot hold a jump", d.at));
        }
        let entry = format!("ArkD{k}");
        let mut new = vec![cmd_label("SCRIPT_WORK_CmdJump", &entry)];
        new.extend((0..(size - jump) / nop).map(|_| cmd("SCRIPT_WORK_CmdNop", &[])));
        // the covered items are replaced as one block at `first`
        at_index.push((first, new));
        for i in first + 1..=last {
            at_index.push((i, Vec::new()));
        }
        let resume = match d.resume {
            Some(r) => {
                let ri = item_of(r)?;
                // reuse a label right before the resume command
                let name = match ri.checked_sub(1).map(|p| &s.items[p]) {
                    Some(Item::Label(l)) => l.clone(),
                    _ => {
                        let l = format!("ArkR{k}");
                        resume_labels.push((ri, l.clone()));
                        l
                    }
                };
                Some(name)
            }
            None => None,
        };
        tail.push(Item::Label(entry));
        for mut it in d.body {
            if let Item::Cmd { args, .. } = &mut it {
                for a in args.iter_mut() {
                    if matches!(a, Value::Label(l) if l == RESUME) {
                        match &resume {
                            Some(r) => *a = Value::Label(r.clone()),
                            None => return err(format!("command {}: the body jumps back but has no resume", d.at)),
                        }
                    }
                }
            }
            tail.push(it);
        }
    }
    for r in replaces {
        let i = item_of(r.at)?;
        if !used.insert(i) {
            return err(format!("command {}: replaced and covered", r.at));
        }
        if size_of(t, &s.items[i])? != size_of(t, &r.with)? {
            return err(format!("command {}: the new command has another size", r.at));
        }
        at_index.push((i, vec![r.with]));
    }
    // rebuild the items: replacements in place, resume labels before their command
    let mut repl: std::collections::BTreeMap<usize, Vec<Item>> = at_index.into_iter().collect();
    let mut labels: std::collections::BTreeMap<usize, Vec<String>> = Default::default();
    for (i, l) in resume_labels {
        labels.entry(i).or_default().push(l);
    }
    let old = std::mem::take(&mut s.items);
    for (i, it) in old.into_iter().enumerate() {
        if let Some(ls) = labels.remove(&i) {
            let mut ls = ls;
            ls.dedup();
            s.items.extend(ls.into_iter().map(Item::Label));
        }
        match repl.remove(&i) {
            Some(new) => s.items.extend(new),
            None => s.items.push(it),
        }
    }
    s.items.extend(tail);
    Ok(used.iter().filter_map(|i| ord_of.get(i).copied()).collect())
}
