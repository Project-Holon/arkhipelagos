//! The check edits: the game's give scripts set a location's flag with a
//! marker instead of giving the item, and the mod gives the world's item
//! (docs/v6-calls.md, "How a check works").
//!
//! At each give the edited script:
//! 1. loads the original item and count into work 0x8008 and 0x8009;
//! 2. loads the gift number (the site's key) into work 0x805E;
//! 3. loads the marker 0xA4C7 into work 0x805F;
//! 4. sets the location's flag (`SetEventFlag`; for trainer gifts
//!    `SetTrainerDefeatedFlag`, which sets flag 1420 + trainer);
//! 5. clears work 0x805F, and sets 0x8010 to 1 (the give succeeded) for
//!    the code after the give.
//!
//! The edits do not depend on the world: every give site is edited, and
//! for a flag that is not in the player's world the mod gives the original
//! item from 0x8008 and 0x8009. Places come from data/world/item_sites.json
//! (tools/locsites.py); the shared routines are found by their commands.

use std::collections::BTreeMap;

use arkh_script::edit::{self, cmd, cmd_label, Detour, Replace, RESUME};
use arkh_script::{Family, Item, Script};
use serde_json::Value;

pub const SCRIPTS: &str = "a/0/5/7";
pub const MARK: u32 = 0xA4C7;
pub const KEY_WORK: u32 = 0x805E;
pub const MARK_WORK: u32 = 0x805F;
pub const BADGE_FLAG: u32 = 370;

const ROUTINES: u32 = 862;
const TRAINERS: u32 = 863;
const BALLS: u32 = 864;

const LOAD_CONST: &str = "SCRIPT_WORK_CmdLoadWorkConst";
const LOAD_VALUE: &str = "SCRIPT_WORK_CmdLoadWorkValue";
const COPY: &str = "SCRIPT_WORK_CmdCopyWork";
const SET_FLAG: &str = "SCRIPT_WORK_CmdSetEventFlag";
const JUMP: &str = "SCRIPT_WORK_CmdJump";

/// Mark, set the flag (`set` is the command that sets it), clear the mark.
fn check(key: u32, set: Item) -> Vec<Item> {
    vec![cmd(LOAD_CONST, &[KEY_WORK, key]), cmd(LOAD_CONST, &[MARK_WORK, MARK]), set, cmd(LOAD_CONST, &[MARK_WORK, 0])]
}

fn ordinal_of_item(s: &Script, item: usize) -> usize {
    s.items[..item].iter().filter(|it| matches!(it, Item::Cmd { .. })).count()
}

fn label_index(s: &Script, label: &str) -> Result<usize, String> {
    s.items.iter().position(|it| matches!(it, Item::Label(l) if l == label)).ok_or(format!("no label {label}"))
}

/// The first command at or after item index `from` with this name and
/// these leading arguments; its ordinal.
fn find(s: &Script, from: usize, name: &str, args: &[u32]) -> Result<usize, String> {
    for (i, it) in s.items.iter().enumerate().skip(from) {
        if let Item::Cmd { name: n, args: a } = it {
            if n == name
                && args.iter().zip(a.iter()).all(|(x, y)| matches!(y, arkh_script::Value::Num(v) if v == x))
            {
                return Ok(ordinal_of_item(s, i));
            }
        }
    }
    Err(format!("no {name} {args:x?}"))
}

fn expect(s: &Script, at: usize, name: &str, args: &[u32]) -> Result<(), String> {
    match edit::cmd_at(s, at) {
        Some((n, a)) if n == name && a.len() >= args.len() && a[..args.len()] == *args => Ok(()),
        got => Err(format!("command {at}: expected {name} {args:x?}, found {got:?}")),
    }
}

fn u(v: &Value) -> Result<u32, String> {
    v.as_u64().map(|x| x as u32).ok_or_else(|| format!("not a number: {v}"))
}

#[derive(Default)]
struct MemberEdits {
    detours: Vec<Detour>,
    replaces: Vec<Replace>,
}

/// Every check edit, built on the player's own members. Returns the
/// changed members (index, bytes).
pub fn members(family: Family, get: &dyn Fn(u32) -> Option<Vec<u8>>) -> Result<Vec<(u32, Vec<u8>)>, String> {
    members_for(family, None, get)
}

/// What a world changes in the scripts besides the checks: the species of
/// the statics, gifts, fossils and starters.
pub struct WorldSpecies<'a> {
    /// "black" or "white"
    pub version: &'a str,
    pub changes: &'a arkh_gen::species::Changes,
    /// the three starters (left, middle, right), empty when unchanged
    pub starters: &'a [u32],
}

/// The check edits, and the world's species edits when a world is given.
pub fn members_for(
    family: Family,
    world: Option<&WorldSpecies>,
    get: &dyn Fn(u32) -> Option<Vec<u8>>,
) -> Result<Vec<(u32, Vec<u8>)>, String> {
    let sites: Value = serde_json::from_str(arkh_gen::data::ITEM_SITES).map_err(|e| e.to_string())?;
    let fk = family.name();
    let mut scripts: BTreeMap<u32, Script> = BTreeMap::new();
    let mut edits: BTreeMap<u32, MemberEdits> = BTreeMap::new();
    let load = |m: u32, scripts: &mut BTreeMap<u32, Script>| -> Result<(), String> {
        if !scripts.contains_key(&m) {
            let d = get(m).ok_or(format!("member {m} missing"))?;
            let s = arkh_script::disassemble(&d, family).map_err(|e| format!("member {m}: {}", e.0))?;
            scripts.insert(m, s);
        }
        Ok(())
    };

    // 862 entry 0 (L003e): an NPC gift with its flag in work 0x8002
    load(ROUTINES, &mut scripts)?;
    {
        let s = &scripts[&ROUTINES];
        let start = label_index(s, &s.entries[0])?;
        let at = find(s, start, "SCRCMD_CanAddItem", &[0x8000, 0x8001, 0x8010])?;
        let resume = find(s, start, "SCRCMD_MSGWIN_CmdBalloonWinMsgTarget", &[0x8006, 0x8004])?;
        let mut body = vec![cmd(COPY, &[0x8008, 0x8000]), cmd(COPY, &[0x8009, 0x8001])];
        body.extend(check(0, cmd(SET_FLAG, &[0x8002])));
        body.push(cmd(LOAD_CONST, &[0x8010, 1]));
        body.push(cmd_label(JUMP, RESUME));
        edits.entry(ROUTINES).or_default().detours.push(Detour { at, cover: 1, body, resume: Some(resume) });

        // 862 entry 11 (L01ae): the hidden items (865 calls it)
        let start = label_index(s, &s.entries[11])?;
        let at = ordinal_of_item(s, start);
        expect(s, at, "SCRIPT_WORK_CmdCall", &[])?;
        let mut body = vec![cmd(COPY, &[0x8008, 0x8000]), cmd(COPY, &[0x8009, 0x8001])];
        body.extend(check(0, cmd(SET_FLAG, &[0x8002])));
        body.push(cmd("SCRIPT_WORK_CmdEnd", &[]));
        edits.entry(ROUTINES).or_default().detours.push(Detour { at, cover: 1, body, resume: None });
    }

    // 864 L23e2: item balls; the flag is the map object's
    load(BALLS, &mut scripts)?;
    {
        let s = &scripts[&BALLS];
        let at = find(s, 0, COPY, &[0x8008, 0x800c])?;
        expect(s, at + 1, COPY, &[0x8009, 0x800d])?;
        let mut body = vec![
            cmd(COPY, &[0x8008, 0x800c]),
            cmd(COPY, &[0x8009, 0x800d]),
            cmd("SCRCMD_FLDMMDL_GetMmdlEventFlag", &[0x8011, 0x8010]),
        ];
        body.extend(check(0, cmd(SET_FLAG, &[0x8010])));
        body.extend([
            cmd("SCRCMD_FLDMMDL_DeleteMmdl", &[0x8011]),
            cmd(LOAD_CONST, &[0x8021, 0]),
            cmd(LOAD_CONST, &[0x8020, 0]),
            cmd("SCRIPT_WORK_CmdReturn", &[]),
        ]);
        edits.entry(BALLS).or_default().detours.push(Detour { at, cover: 1, body, resume: None });
    }

    // 863: the trainers' after-battle gift
    load(TRAINERS, &mut scripts)?;
    for t in sites["trainer_gift"].as_array().ok_or("item_sites.json: trainer_gift")? {
        let at = u(&t["cmd"][fk])? as usize;
        let resume = u(&t["resume"][fk])? as usize;
        expect(&scripts[&TRAINERS], at, LOAD_CONST, &[0x8001, 1])?;
        let mut body = vec![cmd(LOAD_CONST, &[0x8001, 1]), cmd(COPY, &[0x8008, 0x8000]), cmd(LOAD_CONST, &[0x8009, 1])];
        body.extend(check(0, cmd("SCRCMD_WORK_CmdSetTrainerDefeatedFlag", &[0x8020])));
        body.push(cmd(LOAD_CONST, &[0x8010, 1]));
        body.push(cmd_label(JUMP, RESUME));
        edits.entry(TRAINERS).or_default().detours.push(Detour { at, cover: 1, body, resume: Some(resume) });
    }

    // give sites and badges
    let mut done: std::collections::BTreeSet<(u32, usize)> = Default::default();
    let mut badge_members: std::collections::BTreeSet<u32> = Default::default();
    for (_lid, s) in sites["sites"].as_object().ok_or("item_sites.json: sites")? {
        let how = s["how"].as_str().unwrap_or("");
        if how != "give" && how != "badge" {
            continue;
        }
        let flag = u(&s["flag"])?;
        let key = u(&s["key"])?;
        for x in s["sites"].as_array().ok_or("item_sites.json: sites of a site")? {
            let m = u(&x["member"])?;
            let at = u(&x["cmd"][fk])? as usize;
            if !done.insert((m, at)) {
                continue; // one give for several locations
            }
            load(m, &mut scripts)?;
            let sc = &scripts[&m];
            if how == "give" {
                let item = u(&x["item"])?;
                let count = u(&x["count"])?;
                let resume = u(&x["resume"][fk])? as usize;
                match edit::cmd_at(sc, at) {
                    Some((n, a)) if (n == LOAD_VALUE || n == LOAD_CONST) && a == [0x8000, item] => {}
                    got => return Err(format!("member {m} command {at}: expected the give of {item}, found {got:?}")),
                }
                let mut body = vec![
                    cmd(LOAD_VALUE, &[0x8000, item]),
                    cmd(LOAD_VALUE, &[0x8001, count]),
                    cmd(LOAD_CONST, &[0x8008, item]),
                    cmd(LOAD_CONST, &[0x8009, count]),
                ];
                body.extend(check(key, cmd(SET_FLAG, &[flag])));
                body.push(cmd(LOAD_CONST, &[0x8010, 1]));
                body.push(cmd_label(JUMP, RESUME));
                edits.entry(m).or_default().detours.push(Detour { at, cover: 1, body, resume: Some(resume) });
            } else {
                let badge = u(&s["badge"])?;
                expect(sc, at, "SCRCMD_WORK_CmdSetBadge", &[badge])?;
                // SetBadge is 4 bytes: cover it with a neighbour (the one
                // before, or those after) and replay the neighbours in the
                // new code
                let (first, cover) = [(at.wrapping_sub(1), 2), (at, 2), (at, 3), (at.wrapping_sub(1), 3)]
                    .into_iter()
                    .find(|&(f, c)| f != usize::MAX && edit::coverable(sc, family, f, c))
                    .ok_or(format!("member {m} command {at}: no room around SetBadge"))?;
                let ci = edit::cmd_items(sc);
                let mut body = Vec::new();
                let mut ends = false;
                for o in first..first + cover {
                    if o == at {
                        body.extend([cmd(LOAD_CONST, &[0x8008, 0]), cmd(LOAD_CONST, &[0x8009, 0])]);
                        body.extend(check(key, cmd(SET_FLAG, &[flag])));
                    } else {
                        let it = sc.items[ci[o]].clone();
                        if let Item::Cmd { name, .. } = &it {
                            ends = matches!(name.as_str(), "SCRIPT_WORK_CmdEnd" | "SCRIPT_WORK_CmdJump" | "SCRIPT_WORK_CmdHalt");
                            if name.ends_with("CmdIfJump") || name.ends_with("CmdCall") {
                                return Err(format!("member {m} command {o}: a branch next to SetBadge"));
                            }
                        }
                        body.push(it);
                    }
                }
                if !ends {
                    body.push(cmd_label(JUMP, RESUME));
                }
                let resume = if ends { None } else { Some(first + cover) };
                edits.entry(m).or_default().detours.push(Detour { at: first, cover, body, resume });
                badge_members.insert(m);
            }
        }
    }
    // in the gym scripts, "badge N held" means "leader N beaten": the flag
    for (m, list) in sites["checkbadge"].as_object().ok_or("item_sites.json: checkbadge")? {
        let m: u32 = m.parse().map_err(|_| "checkbadge member".to_string())?;
        if !badge_members.contains(&m) {
            continue;
        }
        for x in list.as_array().ok_or("checkbadge list")? {
            let at = u(&x["cmd"][fk])? as usize;
            let badge = u(&x["badge"])?;
            let out = u32::from_str_radix(x["out"].as_str().unwrap_or("").trim_start_matches("0x"), 16)
                .map_err(|_| "checkbadge out".to_string())?;
            expect(&scripts[&m], at, "SCRCMD_WORK_CmdCheckBadge", &[out, badge])?;
            edits.entry(m).or_default().replaces.push(Replace {
                at,
                with: cmd("SCRIPT_WORK_CmdCheckEventFlag", &[BADGE_FLAG + badge, out]),
            });
        }
    }

    if let Some(w) = world {
        for (m, reps) in species_edits(family, w, get)? {
            load(m, &mut scripts)?;
            edits.entry(m).or_default().replaces.extend(reps);
        }
    }

    let mut out = Vec::new();
    for (m, e) in edits {
        let mut s = scripts.remove(&m).ok_or("lost a member")?;
        let orig = get(m).ok_or("lost a member")?;
        let offsets = edit::cmd_offsets(&s, family).map_err(|er| format!("member {m}: {}", er.0))?;
        let entries = s.entries.len();
        let changed = edit::apply(&mut s, family, e.detours, e.replaces).map_err(|er| format!("member {m}: {}", er.0))?;
        if m == ROUTINES {
            add_message_script(&mut s)?;
        }
        let bytes = arkh_script::assemble(&s, family).map_err(|er| format!("member {m}: {}", er.0))?;
        let changed: Vec<(usize, usize)> = changed.iter().map(|&o| offsets[o]).collect();
        verify(m, family, &orig, &bytes, &changed, entries, s.entries.len())?;
        out.push((m, bytes));
    }
    Ok(out)
}

/// The mod's message script, 862 entry 15 (script 2815): "You obtained
/// X!" through the game's own routine (the one 862 entry 4 calls), with the
/// item and count from the saved works 0x40FE and 0x40FF (no script uses
/// them). It starts and ends the way the hidden-item scripts do.
pub const MSG_ENTRY: usize = 15;

fn add_message_script(s: &mut Script) -> Result<(), String> {
    if s.entries.len() != MSG_ENTRY {
        return Err(format!("862 has {} entries, not {MSG_ENTRY}", s.entries.len()));
    }
    let start = label_index(s, &s.entries[4])?;
    let show = match s.items.get(start + 1) {
        Some(Item::Cmd { name, args }) if name == "SCRIPT_WORK_CmdCall" => match args.first() {
            Some(arkh_script::Value::Label(l)) => l.clone(),
            _ => return Err("862 entry 4: no call".into()),
        },
        _ => return Err("862 entry 4: no call".into()),
    };
    s.entries.push("ArkMsg".into());
    s.items.extend([
        Item::Label("ArkMsg".into()),
        cmd("SCRIPT_WORK_CmdCallFieldCommand", &[]),
        cmd(COPY, &[0x8008, 0x40FE]),
        cmd(COPY, &[0x8009, 0x40FF]),
        cmd_label("SCRIPT_WORK_CmdCall", &show),
        cmd("SCRIPT_WORK_CmdCallScriptEvent", &[]),
        cmd("SCRIPT_WORK_CmdPopAllBgm", &[]),
        cmd("SCRIPT_WORK_CmdHalt", &[]),
    ]);
    Ok(())
}

/// An edited member keeps every original byte outside the changed commands
/// (after its entry table, which grows by 4 bytes per added entry, so the
/// code moves by that much), and reads back: disassembled and assembled
/// again it is the same.
fn verify(
    m: u32,
    family: Family,
    orig: &[u8],
    new: &[u8],
    changed: &[(usize, usize)],
    entries_before: usize,
    entries_after: usize,
) -> Result<(), String> {
    let (h0, h1) = (4 * entries_before + 2, 4 * entries_after + 2);
    if new.len() < orig.len() + (h1 - h0) {
        return Err(format!("member {m}: shorter after the edit"));
    }
    for i in 0..entries_before {
        let rel = |d: &[u8], h: usize| i32::from_le_bytes(d[4 * i..4 * i + 4].try_into().unwrap()) as i64 + 4 * i as i64 + 4 - h as i64;
        if rel(orig, h0) != rel(new, h1) {
            return Err(format!("member {m}: entry {i} moved"));
        }
    }
    for i in h0..orig.len() {
        if orig[i] != new[i - h0 + h1] && !changed.iter().any(|&(p, n)| p <= i && i < p + n) {
            return Err(format!("member {m}: byte {i:#x} changed outside the edits"));
        }
    }
    let s = arkh_script::disassemble(new, family).map_err(|e| format!("member {m} after the edit: {}", e.0))?;
    let again = arkh_script::assemble(&s, family).map_err(|e| format!("member {m} after the edit: {}", e.0))?;
    if again != new {
        return Err(format!("member {m}: the edited member does not read back the same"));
    }
    Ok(())
}

/// Where each static, gift and fossil is given or fought (the member
/// that holds the command with its species).
const STATIC_MEMBERS: &[(&str, u32)] = &[
    ("Castelia City Gift Encounter", 94),
    ("Dreamyard Gift Encounter", 304),
    ("Dreamyard Static Encounter", 306),
    ("Rumination Field Static Encounter", 312),
    ("Desert Resort Static Encounter", 316),
    ("Relic Castle Static Encounter", 364),
    ("Dragonspiral Tower Static Encounter", 426),
    ("Dragonspiral Tower Static Encounter", 556),
    ("Trial Chamber Static Encounter", 458),
    ("Giant Chasm Static Encounter", 468),
    ("Liberty Garden Static Encounter", 474),
    ("Marvelous Bridge Sold Encounter", 526),
    ("Guidance Chamber Static Encounter", 670),
    ("Abundant Shrine Static Encounter", 752),
    ("Lostlorn Forest Static Encounter", 770),
    ("Route 18 Egg Encounter", 776),
    ("Reanimation", 877),
    ("Route 6 Item Encounter", 897),
    ("Route 10 Item Encounter", 897),
];

/// The starter script: the choice loads 0x8021 with the species.
const STARTER_MEMBER: u32 = 782;
/// The fossil script: the fossil loads 0x8023 with the species.
const FOSSIL_MEMBER: u32 = 877;

/// The commands that give or fight a Pokemon: (name, species argument).
const GIVES: &[(&str, usize)] = &[
    ("SCRCMD_PROC_FLD_StartScriptedWildBattle", 0),
    ("SCRCMD_PROC_AddMonToPartySimple", 1),
    ("SCRCMD_PROC_AddMonToPartyDetailed", 1),
    ("SCRCMD_PROC_AddMonToBoxSimple", 1),
    ("SCRCMD_PROC_AddEggToParty", 1),
];
/// Other commands that name a species (shown or heard).
const NAMES: &[(&str, usize)] = &[("SCRCMD_BufferSpeciesName", 1), ("SCRCMD_PlayPokeVoice", 0)];

fn num_args(it: &Item) -> Option<(&str, Vec<u32>)> {
    match it {
        Item::Cmd { name, args } => Some((
            name.as_str(),
            args.iter().map(|a| if let arkh_script::Value::Num(n) = a { *n } else { u32::MAX }).collect(),
        )),
        _ => None,
    }
}

/// The species edits of a world, per member: same-size replaces of the
/// species constants.
fn species_edits(
    family: Family,
    w: &WorldSpecies,
    get: &dyn Fn(u32) -> Option<Vec<u8>>,
) -> Result<BTreeMap<u32, Vec<Replace>>, String> {
    let data: Value = serde_json::from_str(arkh_gen::data::SPECIES_LOGIC).map_err(|e| e.to_string())?;
    // member -> [(original species, new species, new form)], in static order
    let mut per: BTreeMap<u32, Vec<(u32, u32, u32)>> = BTreeMap::new();
    for st in data["statics"].as_array().ok_or("species.json statics")? {
        let name = st["name"].as_str().unwrap_or("");
        let Some(new) = w.changes.statics.get(name) else { continue };
        let orig = st[w.version][0].as_u64().ok_or("static species")? as u32;
        if st["kind"] == "starter" || *new == [orig, 0] {
            continue;
        }
        let ms: Vec<u32> = STATIC_MEMBERS.iter().filter(|(n, _)| name.contains(n)).map(|&(_, m)| m).collect();
        if ms.is_empty() && !st["fixed"].as_bool().unwrap_or(false) {
            return Err(format!("no script known for {name}"));
        }
        for m in ms {
            per.entry(m).or_default().push((orig, new[0], new[1]));
        }
    }
    if w.starters.len() == 3 {
        let vanilla: Vec<u32> = data["statics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["kind"] == "starter")
            .filter_map(|s| s[w.version][0].as_u64().map(|x| x as u32))
            .collect();
        for (o, n) in vanilla.iter().zip(w.starters) {
            per.entry(STARTER_MEMBER).or_default().push((*o, *n, 0));
        }
    }
    let mut out: BTreeMap<u32, Vec<Replace>> = BTreeMap::new();
    for (m, list) in per {
        let d = get(m).ok_or(format!("member {m} missing"))?;
        let s = arkh_script::disassemble(&d, family).map_err(|e| format!("member {m}: {}", e.0))?;
        // an original species with several new ones (Desert Resort): the
        // k-th give of it is the k-th static
        let mut by_orig: BTreeMap<u32, Vec<(u32, u32)>> = BTreeMap::new();
        for &(o, n, f) in &list {
            by_orig.entry(o).or_default().push((n, f));
        }
        let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
        let mut reps = Vec::new();
        let mut found: BTreeMap<u32, usize> = BTreeMap::new();
        for (ord, it) in s.items.iter().filter(|it| matches!(it, Item::Cmd { .. })).enumerate() {
            let Some((name, args)) = num_args(it) else { continue };
            let give = GIVES.iter().find(|(n, _)| *n == name).map(|&(_, a)| a);
            let shown = NAMES.iter().find(|(n, _)| *n == name).map(|&(_, a)| a);
            let load = name == LOAD_CONST
                && ((m == STARTER_MEMBER && args[0] == 0x8021) || (m == FOSSIL_MEMBER && args[0] == 0x8023));
            let a = match (give, shown, load) {
                (Some(a), _, _) | (_, Some(a), _) => a,
                (_, _, true) => 1,
                _ => continue,
            };
            let Some(news) = by_orig.get(&args[a]) else { continue };
            let pick = if news.iter().all(|x| *x == news[0]) {
                Some(news[0])
            } else if give.is_some() {
                let k = seen.entry(args[a]).or_default();
                *k += 1;
                news.get(*k - 1).copied()
            } else {
                None // a name of a species that became several: left as it is
            };
            let Some((new, form)) = pick else { continue };
            *found.entry(args[a]).or_default() += 1;
            let mut nargs = args.clone();
            nargs[a] = new;
            if name == "SCRCMD_PROC_AddMonToPartyDetailed" {
                nargs[2] = form;
            }
            reps.push(Replace { at: ord, with: cmd(name, &nargs) });
        }
        for o in by_orig.keys() {
            if !found.contains_key(o) {
                return Err(format!("member {m}: species {o} not found"));
            }
        }
        out.insert(m, reps);
    }
    Ok(out)
}
