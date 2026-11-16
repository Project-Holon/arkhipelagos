//! Species in the logic: where each player can catch or see which Pokemon.
//!
//! - `SpeciesModel`: the places (wild areas, statics, trades, trainers, form
//!   changes) with their compiled rules; the same for every player.
//! - `Tables`: one player's game content (the original tables, or the
//!   randomized ones): what is found at each place, the species' types and
//!   machines, the evolutions.
//! - `PlayerSpecies`: the two joined, ready for the sweep: each source with
//!   its regions, rule and mons; each evolution with its rule; the species
//!   sets and the per-world picks.
//!
//! A mon is a (species, form) pair (`Facts::mons`); "caught" is per mon,
//! "seen" per species.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::data::{Data, Rule};
use crate::facts::{EncounterBlock, Personal, Trainer, LAST_DEX};
use crate::model::Model;
use crate::rng::Rng;

pub struct WildDef {
    pub area: String,
    pub regions: Vec<u32>,
    pub include: Rule,
    pub rule: Rule,
    pub file: u32,
    pub season: Option<u32>,
    pub method: String,
}

pub struct StaticDef {
    pub kind: String,
    pub name: String,
    pub slot: u32,
    /// [dex, form] in Black and in White
    pub vanilla: [[u32; 2]; 2],
    pub region: u32,
    pub include: Rule,
    pub rule: Rule,
    pub fixed: bool,
}

pub struct TradeDef {
    pub name: String,
    pub vanilla: [[u32; 2]; 2],
    pub wanted: [u32; 2],
    pub region: u32,
}

pub struct TrainerDef {
    pub id: u32,
    /// the gym town (or "League N") of a gym trainer
    pub gym: Option<String>,
    pub region: Option<u32>,
    pub include: Rule,
    pub rule: Rule,
    pub boss: bool,
    pub rival: u32,
}

pub struct FormDef {
    pub dex: u32,
    pub region: u32,
    pub include: Rule,
    pub forms: Vec<Rule>,
}

pub enum SetDef {
    /// can learn the machine (compatibility bit)
    Machine(u8),
    /// has the type
    Type(u8),
    /// has the type picked for the Castelia studio
    StudioType,
}

#[derive(Default)]
pub struct SpeciesModel {
    pub wild: Vec<WildDef>,
    pub statics: Vec<StaticDef>,
    pub trades: Vec<TradeDef>,
    pub trainers: Vec<TrainerDef>,
    pub forms: Vec<FormDef>,
    pub sets: Vec<SetDef>,
    pub evo_methods: BTreeMap<u32, Value>,
    pub evo_items: BTreeMap<u32, String>,
    pub hall: Option<u32>,
    pub legendaries: Vec<u32>,
}

impl SpeciesModel {
    pub fn build(m: &Model, d: &Data) -> Result<SpeciesModel, String> {
        let c = |v: &Value, what: &str| m.compile(v).map_err(|e| format!("{what}: {e}"));
        let mut s = SpeciesModel::default();
        for w in &d.species.wild {
            let what = format!("wild {} {}", w.area, w.method);
            s.wild.push(WildDef {
                area: w.area.clone(),
                regions: w.regions.iter().map(|r| m.region(r, &what)).collect::<Result<_, _>>()?,
                include: c(&w.include, &what)?,
                rule: c(&w.rule, &what)?,
                file: w.file,
                season: w.season,
                method: w.method.clone(),
            });
        }
        for x in &d.species.statics {
            s.statics.push(StaticDef {
                kind: x.kind.clone(),
                name: x.name.clone(),
                slot: x.slot,
                vanilla: [x.black, x.white],
                region: m.region(&x.region, &x.name)?,
                include: c(&x.include, &x.name)?,
                rule: c(&x.rule, &x.name)?,
                fixed: x.fixed,
            });
        }
        for x in &d.species.trades {
            s.trades.push(TradeDef {
                name: x.name.clone(),
                vanilla: [x.black, x.white],
                wanted: [x.wanted_black, x.wanted_white],
                region: m.region(&x.region, &x.name)?,
            });
        }
        for t in &d.species.trainers {
            let what = format!("trainer {}", t.id);
            s.trainers.push(TrainerDef {
                id: t.id,
                gym: t.gym.as_ref().and_then(|g| g.get(0)).and_then(|x| x.as_str()).map(|x| x.to_string()),
                region: if t.region.is_empty() { None } else { Some(m.region(&t.region, &what)?) },
                include: c(&t.include, &what)?,
                rule: c(&t.rule, &what)?,
                boss: t.boss,
                rival: t.rival,
            });
        }
        for f in &d.species_rules.form_changes {
            let what = format!("form change {}", f.dex);
            s.forms.push(FormDef {
                dex: f.dex,
                region: m.region(&f.region, &what)?,
                include: c(&f.include, &what)?,
                forms: f.forms.iter().map(|r| c(r, &what)).collect::<Result<_, _>>()?,
            });
        }
        for name in &m.set_names {
            let v = &d.species_rules.sets[name];
            let def = if let Some(mc) = v.get("machine").and_then(|x| x.as_str()) {
                let mach = m.facts.machine(mc).ok_or_else(|| format!("set {name}: no machine {mc}"))?;
                SetDef::Machine(mach.compat_bit)
            } else if let Some(t) = v.get("type").and_then(|x| x.as_u64()) {
                SetDef::Type(t as u8)
            } else if v.get("type").and_then(|x| x.as_str()) == Some("studio_castelia_type") {
                SetDef::StudioType
            } else {
                return Err(format!("set {name}: unknown form {v}"));
            };
            s.sets.push(def);
        }
        for (k, v) in &d.species_rules.evolution_methods {
            s.evo_methods.insert(k.parse().map_err(|_| format!("evolution method {k}"))?, v.clone());
        }
        for (k, v) in &d.species_rules.evolution_items {
            s.evo_items.insert(k.parse().map_err(|_| format!("evolution item {k}"))?, v.clone());
        }
        s.hall = m.region_by_name.get("Hall of Fame").copied();
        s.legendaries = d.species_rules.legendaries.clone();
        Ok(s)
    }
}

/// One player's game content: the original tables or the randomized ones.
#[derive(Clone)]
pub struct Tables {
    /// 0 Black, 1 White
    pub version: usize,
    pub personal: Vec<Personal>,
    pub learnsets: Vec<Vec<[u16; 2]>>,
    pub evolutions: Vec<Vec<[u16; 3]>>,
    pub wild: Vec<Vec<EncounterBlock>>,
    /// per SpeciesModel::statics: [dex, form]
    pub statics: Vec<[u32; 2]>,
    /// per SpeciesModel::trades: ([dex, form] given, dex wanted)
    pub trades: Vec<([u32; 2], u32)>,
    pub trainers: Vec<Option<Trainer>>,
    /// per machine (facts order): the move it teaches
    pub machine_moves: Vec<u32>,
    pub moves: Vec<Option<crate::facts::Move>>,
    pub type_chart: Vec<Vec<u8>>,
}

impl Tables {
    pub fn vanilla(m: &Model, version: usize) -> Tables {
        let f = &m.facts;
        Tables {
            version,
            personal: f.species.clone(),
            learnsets: f.learnsets.clone(),
            evolutions: f.evolutions.clone(),
            wild: f.encounters[version].clone(),
            statics: m.species.statics.iter().map(|s| s.vanilla[version]).collect(),
            trades: m.species.trades.iter().map(|t| (t.vanilla[version], t.wanted[version])).collect(),
            trainers: f.trainers.clone(),
            machine_moves: f.machines.iter().map(|x| x.mv).collect(),
            moves: f.moves.clone(),
            type_chart: f.type_chart.clone(),
        }
    }

    pub fn moves_ok(&self, mv: u32) -> bool {
        matches!(self.moves.get(mv as usize), Some(Some(_)))
    }

    pub fn move_type(&self, mv: u32) -> u8 {
        self.moves.get(mv as usize).cloned().flatten().map(|m| m.kind).unwrap_or(0)
    }

    pub fn personal_of(&self, m: &Model, mon: u32) -> &Personal {
        &self.personal[m.facts.record(mon)]
    }

    /// The mons of a wild source: every slot of its method in its season.
    pub fn wild_slots(&self, file: u32, season: Option<u32>, method: &str) -> &[[u16; 4]] {
        let blocks = match self.wild.get(file as usize) {
            Some(b) if !b.is_empty() => b,
            _ => return &[],
        };
        let b = &blocks[(season.unwrap_or(0) as usize).min(blocks.len() - 1)];
        b.slots.get(method).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// What the randomizers changed in one player's game: only the entries
/// that differ from the original tables. The mod turns these into the
/// player's game files.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Changes {
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub personal: BTreeMap<u32, Personal>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub learnsets: BTreeMap<u32, Vec<[u16; 2]>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub evolutions: BTreeMap<u32, Vec<[u16; 3]>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub moves: BTreeMap<u32, crate::facts::Move>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub type_chart: Option<Vec<Vec<u8>>>,
    /// encounter file -> its season blocks
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub wild: BTreeMap<u32, Vec<EncounterBlock>>,
    /// static encounter name -> [species, form]
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub statics: BTreeMap<String, [u32; 2]>,
    /// trade name -> ([species, form] offered, species wanted)
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub trades: BTreeMap<String, ([u32; 2], u32)>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub trainers: BTreeMap<u32, Trainer>,
}

impl Changes {
    /// The tables these changes make from the original ones (the inverse of
    /// `of`).
    pub fn apply(&self, m: &Model, t: &mut Tables) {
        for (i, p) in &self.personal {
            if let Some(x) = t.personal.get_mut(*i as usize) {
                *x = p.clone();
            }
        }
        for (i, v) in &self.learnsets {
            if let Some(x) = t.learnsets.get_mut(*i as usize) {
                *x = v.clone();
            }
        }
        for (i, v) in &self.evolutions {
            if let Some(x) = t.evolutions.get_mut(*i as usize) {
                *x = v.clone();
            }
        }
        for (i, v) in &self.moves {
            if let Some(x) = t.moves.get_mut(*i as usize) {
                *x = Some(v.clone());
            }
        }
        if let Some(c) = &self.type_chart {
            t.type_chart = c.clone();
        }
        for (i, v) in &self.wild {
            if let Some(x) = t.wild.get_mut(*i as usize) {
                *x = v.clone();
            }
        }
        for (k, s) in m.species.statics.iter().enumerate() {
            if let Some(v) = self.statics.get(&s.name) {
                t.statics[k] = *v;
            }
        }
        for (k, tr) in m.species.trades.iter().enumerate() {
            if let Some(v) = self.trades.get(&tr.name) {
                t.trades[k] = *v;
            }
        }
        for (i, v) in &self.trainers {
            if let Some(x) = t.trainers.get_mut(*i as usize) {
                *x = Some(v.clone());
            }
        }
    }

    pub fn of(m: &Model, t: &Tables) -> Changes {
        let v = Tables::vanilla(m, t.version);
        let mut c = Changes::default();
        for (i, (a, b)) in t.personal.iter().zip(&v.personal).enumerate().skip(1) {
            if a != b {
                c.personal.insert(i as u32, a.clone());
            }
        }
        for (i, (a, b)) in t.learnsets.iter().zip(&v.learnsets).enumerate() {
            if a != b {
                c.learnsets.insert(i as u32, a.clone());
            }
        }
        for (i, (a, b)) in t.evolutions.iter().zip(&v.evolutions).enumerate() {
            if a != b {
                c.evolutions.insert(i as u32, a.clone());
            }
        }
        for (i, (a, b)) in t.moves.iter().zip(&v.moves).enumerate() {
            if let (Some(a), true) = (a, a != b) {
                c.moves.insert(i as u32, a.clone());
            }
        }
        if t.type_chart != v.type_chart {
            c.type_chart = Some(t.type_chart.clone());
        }
        for (i, (a, b)) in t.wild.iter().zip(&v.wild).enumerate() {
            if a != b {
                c.wild.insert(i as u32, a.clone());
            }
        }
        for (i, s) in m.species.statics.iter().enumerate() {
            if t.statics[i] != v.statics[i] {
                c.statics.insert(s.name.clone(), t.statics[i]);
            }
        }
        for (i, tr) in m.species.trades.iter().enumerate() {
            if t.trades[i] != v.trades[i] {
                c.trades.insert(tr.name.clone(), t.trades[i]);
            }
        }
        for (i, (a, b)) in t.trainers.iter().zip(&v.trainers).enumerate() {
            if let (Some(a), true) = (a, a != b) {
                c.trainers.insert(i as u32, a.clone());
            }
        }
        c
    }
}

pub struct Source {
    pub regions: Vec<u32>,
    pub on: bool,
    pub rule: Rule,
    pub mons: Vec<u32>,
    /// trainers: the species are seen, not caught
    pub seen: bool,
    pub level: u32,
}

pub struct Evo {
    pub base: u32,
    pub target: u32,
    pub rule: Rule,
}

pub struct FormGroup {
    pub region: u32,
    pub mons: Vec<u32>,
    pub rules: Vec<Rule>,
}

/// The per-world picks some locations ask for.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Picks {
    /// [dex, form] the Route 5 and Driftveil people ask to see
    pub other_locations_species: Option<[u32; 2]>,
    /// the type the Castelia studio asks for
    pub studio_castelia_type: Option<u32>,
    /// the item number of the TM the Driftveil man asks about
    pub driftveil_tm: Option<u32>,
}

pub struct PlayerSpecies {
    pub sources: Vec<Source>,
    pub evos: Vec<Evo>,
    pub forms: Vec<FormGroup>,
    /// per mon: the species sets it is in
    pub sets_of: Vec<Vec<u32>>,
    pub picks: Picks,
    pub other_mon: Option<u32>,
    /// item index of the Driftveil TM
    pub tm_item: Option<u32>,
    pub monkeys_not_split: bool,
    pub hall: Option<u32>,
    /// dexcountsanity: a count check needs this many more species ...
    pub count_leniency: u32,
    /// ... but never more than this many (the species the player can catch)
    pub count_cap: u32,
}

impl PlayerSpecies {
    /// Only the flags rules read before the player's species are built.
    pub fn flags(m: &Model, t: &Tables) -> PlayerSpecies {
        PlayerSpecies {
            sources: Vec::new(),
            evos: Vec::new(),
            forms: Vec::new(),
            sets_of: vec![Vec::new(); m.facts.mons.len()],
            picks: Picks::default(),
            other_mon: None,
            tm_item: None,
            monkeys_not_split: monkeys_not_split(m, t),
            hall: m.species.hall,
            count_leniency: 0,
            count_cap: u32::MAX,
        }
    }
}

/// The three Dreamyard monkeys are one species (only when the gifts are
/// randomized to the same Pokemon).
fn monkeys_not_split(m: &Model, t: &Tables) -> bool {
    let idx: Vec<usize> = m
        .species
        .statics
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kind == "gift" && s.name.starts_with("Dreamyard Gift Encounter"))
        .map(|(i, _)| i)
        .collect();
    idx.len() == 3 && idx.iter().all(|&i| t.statics[i] == t.statics[idx[0]])
}

fn mon_of(m: &Model, dex: u32, form: u32) -> Option<u32> {
    m.facts.mon(dex, form).or_else(|| m.facts.mon(dex, 0))
}

/// Which species sources the logic counts (option "species_sources").
pub struct Use {
    pub statics: bool,
    pub trades: bool,
    pub evolutions: bool,
    pub form_change: bool,
    pub trainers: bool,
}

/// The rule of one evolution row for a base record, in the rule language.
fn evo_rule(m: &Model, t: &Tables, base_rec: usize, method: u32, param: u32) -> Value {
    let tmpl = match m.species.evo_methods.get(&method) {
        Some(v) => v.clone(),
        None => return Value::Bool(false),
    };
    expand(m, t, base_rec, &tmpl, param)
}

fn expand(m: &Model, t: &Tables, base_rec: usize, v: &Value, param: u32) -> Value {
    let p = |x: &Value| -> u32 {
        if x.as_str() == Some("param") {
            param
        } else {
            x.as_u64().unwrap_or(0) as u32
        }
    };
    match v {
        Value::Object(o) => {
            if let Some(x) = o.get("level") {
                return json!({ "level": p(x) });
            }
            if let Some(x) = o.get("evo_item") {
                return match m.species.evo_items.get(&p(x)) {
                    Some(r) => json!({ "rule": r }),
                    None => Value::Bool(false),
                };
            }
            if let Some(x) = o.get("party") {
                return json!({ "species": [p(x), 0] });
            }
            if let Some(x) = o.get("knows_move") {
                let mv = p(x);
                // learned by level: that level and the move relearner
                if let Some(l) = t.learnsets[base_rec].iter().find(|e| e[0] as u32 == mv) {
                    return json!({ "all": [ { "level": l[1] }, { "rule": "has_access_move_relearner" } ] });
                }
                // taught by a machine the species can use: that machine
                for (i, mach) in m.facts.machines.iter().enumerate() {
                    if t.machine_moves[i] == mv && t.personal[base_rec].machines.contains(&mach.compat_bit) {
                        return json!({ "has_id": mach.item });
                    }
                }
                return json!({ "rule": "has_access_move_relearner" });
            }
            Value::Object(o.iter().map(|(k, x)| (k.clone(), expand(m, t, base_rec, x, param))).collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| expand(m, t, base_rec, x, param)).collect()),
        x => x.clone(),
    }
}

/// Join the places with this player's tables. `on` tells whether an
/// inclusion rule holds for the player.
pub fn build(
    m: &Model,
    t: &Tables,
    use_: &Use,
    on: &dyn Fn(&Rule) -> bool,
    rng: &mut Rng,
    given: Option<&Picks>,
) -> Result<PlayerSpecies, String> {
    let sm = &m.species;
    let mut ps = PlayerSpecies::flags(m, t);
    let mut catchable = vec![false; m.facts.mons.len()];

    for w in &sm.wild {
        let mut mons = Vec::new();
        let mut level = 0;
        for s in t.wild_slots(w.file, w.season, &w.method) {
            if s[0] == 0 {
                continue;
            }
            if let Some(mon) = mon_of(m, s[0] as u32, s[1] as u32) {
                if !mons.contains(&mon) {
                    mons.push(mon);
                }
                level = level.max((s[2] as u32 + s[3] as u32) / 2);
            }
        }
        let is_on = on(&w.include) && !mons.is_empty();
        if is_on {
            for &x in &mons {
                catchable[x as usize] = true;
            }
        }
        ps.sources.push(Source { regions: w.regions.clone(), on: is_on, rule: w.rule.clone(), mons, seen: false, level });
    }
    for (i, s) in sm.statics.iter().enumerate() {
        let [d, f] = t.statics[i];
        let mons: Vec<u32> = mon_of(m, d, f).into_iter().collect();
        let is_on = use_.statics && on(&s.include) && !mons.is_empty();
        if is_on {
            for &x in &mons {
                catchable[x as usize] = true;
            }
        }
        ps.sources.push(Source { regions: vec![s.region], on: is_on, rule: s.rule.clone(), mons, seen: false, level: 0 });
    }
    for (i, tr) in sm.trades.iter().enumerate() {
        let ([d, f], wanted) = t.trades[i];
        let mons: Vec<u32> = mon_of(m, d, f).into_iter().collect();
        let rule = match mon_of(m, wanted, 0) {
            Some(w) => Rule::Caught(w),
            None => Rule::Const(false),
        };
        let is_on = use_.trades && !mons.is_empty();
        if is_on {
            for &x in &mons {
                catchable[x as usize] = true;
            }
        }
        ps.sources.push(Source { regions: vec![tr.region], on: is_on, rule, mons, seen: false, level: 0 });
    }
    for tr in &sm.trainers {
        let party = t.trainers.get(tr.id as usize).cloned().flatten();
        let (Some(region), Some(party)) = (tr.region, party) else { continue };
        let mut mons = Vec::new();
        let mut level = 0;
        for p in &party.party {
            if let Some(x) = mon_of(m, p.species as u32, p.form as u32) {
                if !mons.contains(&x) {
                    mons.push(x);
                }
            }
            level = level.max(p.level as u32);
        }
        // a trainer's level counts even when its species do not
        let is_on = on(&tr.include);
        let seen_on = is_on && use_.trainers;
        ps.sources.push(Source {
            regions: vec![region],
            on: is_on,
            rule: tr.rule.clone(),
            mons: if seen_on { mons } else { Vec::new() },
            seen: true,
            level,
        });
    }

    // evolutions: every row of every mon (the sweep only follows caught ones)
    if use_.evolutions {
        for mon in 0..m.facts.mons.len() as u32 {
            let rec = m.facts.record(mon);
            let (_, form) = m.facts.mons[mon as usize];
            for row in &t.evolutions[rec] {
                let [method, param, target] = *row;
                let Some(tgt) = mon_of(m, target as u32, form as u32) else { continue };
                let v = evo_rule(m, t, rec, method as u32, param as u32);
                let rule = m.compile(&v).map_err(|e| format!("evolution {rec} -> {target}: {e}"))?;
                if matches!(rule, Rule::Const(false)) {
                    continue;
                }
                ps.evos.push(Evo { base: mon, target: tgt, rule });
            }
        }
        // catchable, for the picks: follow evolutions without their rules
        loop {
            let mut grew = false;
            for e in &ps.evos {
                if catchable[e.base as usize] && !catchable[e.target as usize] {
                    catchable[e.target as usize] = true;
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
    }
    if use_.form_change {
        for fd in &sm.forms {
            if !on(&fd.include) {
                continue;
            }
            let mons: Vec<u32> = (0..fd.forms.len() as u32).filter_map(|f| m.facts.mon(fd.dex, f)).collect();
            ps.forms.push(FormGroup { region: fd.region, mons, rules: fd.forms.clone() });
        }
    }

    // the per-world picks (BWAP: a catchable species with machines, not a form)
    let cands: Vec<u32> = (0..m.facts.mons.len() as u32)
        .filter(|&x| catchable[x as usize] && m.facts.mons[x as usize].1 == 0)
        .filter(|&x| !t.personal_of(m, x).machines.is_empty())
        .collect();
    if let Some(p) = given {
        // the picks a world was made with (the mod rebuilding a player's logic)
        ps.picks = p.clone();
        ps.other_mon = p.other_locations_species.and_then(|[d, f]| mon_of(m, d, f));
        ps.tm_item = p.driftveil_tm.and_then(|i| m.item_by_id.get(&i).copied());
    } else if !cands.is_empty() {
        let mon = *rng.pick(&cands);
        let (dex, form) = m.facts.mons[mon as usize];
        let per = t.personal_of(m, mon);
        let ty = if per.types[0] == per.types[1] { per.types[0] } else { per.types[rng.below(2) as usize] };
        let mut machs: Vec<&crate::facts::Machine> =
            m.facts.machines.iter().filter(|x| per.machines.contains(&x.compat_bit)).collect();
        machs.sort_by(|a, b| a.name.cmp(&b.name));
        let mach = machs[rng.below(machs.len() as u64) as usize];
        ps.picks = Picks {
            other_locations_species: Some([dex as u32, form as u32]),
            studio_castelia_type: Some(ty as u32),
            driftveil_tm: Some(mach.item),
        };
        ps.other_mon = Some(mon);
        ps.tm_item = m.item_by_id.get(&mach.item).copied();
    }

    // species sets
    for mon in 0..m.facts.mons.len() {
        let per = t.personal_of(m, mon as u32);
        for (si, sd) in m.species.sets.iter().enumerate() {
            let inside = match sd {
                SetDef::Machine(b) => per.machines.contains(b),
                SetDef::Type(ty) => per.types.contains(ty),
                SetDef::StudioType => ps.picks.studio_castelia_type.map(|x| per.types.contains(&(x as u8))) == Some(true),
            };
            if inside {
                ps.sets_of[mon].push(si as u32);
            }
        }
    }
    let _ = LAST_DEX;
    Ok(ps)
}

/// One player's species state in a sweep.
#[derive(Clone)]
pub struct SpState {
    pub caught: Vec<bool>,
    pub seen_dex: Vec<bool>,
    pub seen_n: u32,
    pub set_hit: Vec<bool>,
    /// species (National Dex numbers) caught, any form
    pub caught_dex: Vec<bool>,
    pub caught_dex_n: u32,
    pub level: u32,
    pub src_done: Vec<bool>,
    pub evo_done: Vec<bool>,
}

impl SpState {
    pub fn new(m: &Model) -> SpState {
        SpState {
            caught: vec![false; m.facts.mons.len()],
            seen_dex: vec![false; LAST_DEX as usize + 1],
            seen_n: 0,
            set_hit: vec![false; m.set_names.len()],
            caught_dex: vec![false; LAST_DEX as usize + 1],
            caught_dex_n: 0,
            level: 0,
            src_done: Vec::new(),
            evo_done: Vec::new(),
        }
    }

    pub fn see(&mut self, dex: u32) {
        if !self.seen_dex[dex as usize] {
            self.seen_dex[dex as usize] = true;
            self.seen_n += 1;
        }
    }

    pub fn catch(&mut self, m: &Model, ps: &PlayerSpecies, mon: u32) -> bool {
        if self.caught[mon as usize] {
            return false;
        }
        self.caught[mon as usize] = true;
        let dex = m.facts.mons[mon as usize].0 as usize;
        if dex < self.caught_dex.len() && !self.caught_dex[dex] {
            self.caught_dex[dex] = true;
            self.caught_dex_n += 1;
        }
        self.see(dex as u32);
        for &s in &ps.sets_of[mon as usize] {
            self.set_hit[s as usize] = true;
        }
        true
    }
}
