//! Turn one player's table changes (`species::Changes` in a world) into
//! game members, built on the player's own members: every byte the change
//! does not name is kept from the cartridge, so no game data ships with the
//! mod. The record layouts are the ones tools/facts.py reads (docs/facts.md).
//!
//! The eidos mod calls `members` for each archive and gives the results to
//! `vfs_member_set`; the type chart goes to `type_chart_set`.

use arkh_gen::facts::{EncounterBlock, Move, Personal, Trainer};
use arkh_gen::species::Changes;

pub const PERSONAL: &str = "a/0/1/6";
pub const LEARNSETS: &str = "a/0/1/8";
pub const EVOLUTIONS: &str = "a/0/1/9";
pub const MOVES: &str = "a/0/2/1";
pub const ENCOUNTERS: &str = "a/1/2/6";
pub const TRAINER_PARTIES: &str = "a/0/9/3";

/// The archives a change can touch, in a fixed order.
pub const ARCHIVES: [&str; 6] = [PERSONAL, LEARNSETS, EVOLUTIONS, MOVES, ENCOUNTERS, TRAINER_PARTIES];

const METHODS: [(&str, usize); 7] = [
    ("grass", 12),
    ("double_grass", 12),
    ("shaking_grass", 12),
    ("surf", 5),
    ("surf_ripple", 5),
    ("fishing", 5),
    ("fishing_ripple", 5),
];

fn put16(d: &mut [u8], at: usize, v: u16) {
    d[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn get16(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

/// A personal record (60 bytes): the fields `Personal` holds, the rest kept.
pub fn personal(orig: &[u8], p: &Personal) -> Result<Vec<u8>, String> {
    if orig.len() != 60 {
        return Err(format!("personal record of {} bytes", orig.len()));
    }
    let mut d = orig.to_vec();
    d[0..6].copy_from_slice(&p.stats);
    d[6] = p.types[0];
    d[7] = p.types[1];
    d[8] = p.catch_rate;
    for (i, it) in p.items.iter().enumerate() {
        put16(&mut d, 0x0C + 2 * i, *it);
    }
    d[0x12] = p.gender;
    d[0x13] = p.hatch;
    d[0x14] = p.friendship;
    d[0x15] = p.growth;
    d[0x16] = p.egg_groups[0];
    d[0x17] = p.egg_groups[1];
    d[0x18..0x1B].copy_from_slice(&p.abilities);
    let mut words = [0u32; 4];
    for b in &p.machines {
        let b = *b as usize;
        if b > 127 {
            return Err(format!("machine bit {b}"));
        }
        words[b / 32] |= 1 << (b % 32);
    }
    for (i, w) in words.iter().enumerate() {
        d[0x28 + 4 * i..0x2C + 4 * i].copy_from_slice(&w.to_le_bytes());
    }
    Ok(d)
}

/// A learnset: (move, level) pairs ended by 0xFFFF 0xFFFF.
pub fn learnset(moves: &[[u16; 2]]) -> Vec<u8> {
    let mut d = Vec::with_capacity(4 * moves.len() + 4);
    for e in moves.iter().chain(std::iter::once(&[0xFFFF, 0xFFFF])) {
        d.extend_from_slice(&e[0].to_le_bytes());
        d.extend_from_slice(&e[1].to_le_bytes());
    }
    d
}

/// An evolution record: 7 rows of (method, parameter, target), unused rows 0.
pub fn evolution(orig: &[u8], rows: &[[u16; 3]]) -> Result<Vec<u8>, String> {
    if rows.len() > 7 {
        return Err(format!("{} evolution rows", rows.len()));
    }
    let mut d = orig.to_vec();
    d.resize(42.max(orig.len()), 0);
    for i in 0..7 {
        let r = rows.get(i).copied().unwrap_or([0, 0, 0]);
        for (k, v) in r.iter().enumerate() {
            put16(&mut d, 6 * i + 2 * k, *v);
        }
    }
    Ok(d)
}

/// A move record (36 bytes): type, damage type, power, accuracy, PP,
/// priority and ailment; the rest kept.
pub fn move_record(orig: &[u8], m: &Move) -> Result<Vec<u8>, String> {
    if orig.len() != 36 {
        return Err(format!("move record of {} bytes", orig.len()));
    }
    let mut d = orig.to_vec();
    d[0] = m.kind;
    d[1] = m.group;
    d[2] = m.damage_type;
    d[3] = m.power;
    d[4] = m.accuracy;
    d[5] = m.pp;
    d[6] = m.priority as u8;
    put16(&mut d, 8, m.ailment);
    Ok(d)
}

/// An encounter file: each 232-byte season block's slots rewritten; the
/// rate bytes kept.
pub fn encounters(orig: &[u8], blocks: &[EncounterBlock]) -> Result<Vec<u8>, String> {
    if orig.len() != 232 * blocks.len() {
        return Err(format!("encounter file of {} bytes for {} blocks", orig.len(), blocks.len()));
    }
    let mut d = orig.to_vec();
    for (b, blk) in blocks.iter().enumerate() {
        let mut k = 0usize;
        for (method, n) in METHODS {
            let slots = blk.slots.get(method).ok_or_else(|| format!("no {method} slots"))?;
            if slots.len() != n {
                return Err(format!("{method}: {} slots", slots.len()));
            }
            for s in slots {
                let at = 232 * b + 8 + 4 * k;
                put16(&mut d, at, (s[0] & 0x7FF) | (s[1] << 11));
                d[at + 2] = s[2] as u8;
                d[at + 3] = s[3] as u8;
                k += 1;
            }
        }
    }
    Ok(d)
}

/// A trainer's party (a/0/9/3): entries of 8, 16, 10 or 18 bytes by the
/// party type, read from the entry size; the count is kept.
pub fn party(orig: &[u8], t: &Trainer) -> Result<Vec<u8>, String> {
    let n = t.party.len();
    if n == 0 {
        return Ok(orig.to_vec());
    }
    let size = orig.len() / n;
    let (has_item, has_moves) = match size {
        8 => (false, false),
        16 => (false, true),
        10 => (true, false),
        18 => (true, true),
        _ => return Err(format!("party of {} bytes for {n} Pokemon", orig.len())),
    };
    let mut d = orig.to_vec();
    for (k, m) in t.party.iter().enumerate() {
        let at = size * k;
        d[at] = m.dv;
        d[at + 1] = m.ability;
        put16(&mut d, at + 2, m.level);
        put16(&mut d, at + 4, m.species);
        put16(&mut d, at + 6, m.form);
        let mut o = at + 8;
        if has_item {
            put16(&mut d, o, m.item.unwrap_or_else(|| get16(orig, o)));
            o += 2;
        }
        if has_moves {
            let mv = m.moves.unwrap_or([get16(orig, o), get16(orig, o + 2), get16(orig, o + 4), get16(orig, o + 6)]);
            for (i, x) in mv.iter().enumerate() {
                put16(&mut d, o + 2 * i, *x);
            }
        }
    }
    Ok(d)
}

/// The changed members of one archive: (member index, new bytes), built on
/// `get(member)`, the player's own member.
pub fn members(
    c: &Changes,
    archive: &str,
    get: &dyn Fn(u32) -> Option<Vec<u8>>,
) -> Result<Vec<(u32, Vec<u8>)>, String> {
    let orig = |i: u32| get(i).ok_or_else(|| format!("{archive}: no member {i}"));
    let mut out = Vec::new();
    match archive {
        PERSONAL => {
            for (i, p) in &c.personal {
                out.push((*i, personal(&orig(*i)?, p)?));
            }
        }
        LEARNSETS => {
            for (i, l) in &c.learnsets {
                out.push((*i, learnset(l)));
            }
        }
        EVOLUTIONS => {
            for (i, e) in &c.evolutions {
                out.push((*i, evolution(&orig(*i)?, e)?));
            }
        }
        MOVES => {
            for (i, m) in &c.moves {
                out.push((*i, move_record(&orig(*i)?, m)?));
            }
        }
        ENCOUNTERS => {
            for (i, b) in &c.wild {
                out.push((*i, encounters(&orig(*i)?, b)?));
            }
        }
        TRAINER_PARTIES => {
            for (i, t) in &c.trainers {
                out.push((*i, party(&orig(*i)?, t)?));
            }
        }
        _ => return Err(format!("no table in {archive}")),
    }
    Ok(out)
}

/// The type chart as the game stores it (17 x 17 bytes), for type_chart_set.
pub fn type_chart(c: &Changes) -> Option<Vec<u8>> {
    c.type_chart.as_ref().map(|t| t.iter().flatten().copied().collect())
}
