//! The randomizers: they change one player's `Tables` before the logic is
//! built on them. Integer arithmetic and ordered collections only, and each
//! randomizer has its own random stream, so one option never reshuffles
//! another.
//!
//! Options are lists of modifiers (as in BWAP): an empty list leaves that
//! part of the game as it is; "randomize" (or "shuffle") turns it on.

use std::collections::BTreeMap;

use crate::facts::LAST_DEX;
use crate::gen::Opts;
use crate::model::Model;
use crate::rng::Rng;
use crate::species::Tables;

/// The highest move number (Fusion Bolt is 559) and ability number.
const LAST_MOVE: u32 = 559;
const LAST_ABILITY: u32 = 164;
const STRUGGLE: u32 = 165;
const TYPES: u8 = 17;

/// Evolution families, stages and the legendary list.
pub struct Families {
    /// dex -> pre-evolution dex
    pub pre: Vec<Option<u32>>,
    /// dex -> the first species of its family
    pub root: Vec<u32>,
    /// dex -> has an evolution
    pub evolves: Vec<bool>,
}

impl Families {
    pub fn new(m: &Model) -> Families {
        let n = LAST_DEX as usize + 1;
        let mut pre = vec![None; n];
        let mut evolves = vec![false; n];
        for dex in 1..=LAST_DEX as usize {
            for row in &m.facts.evolutions[dex] {
                let t = row[2] as usize;
                if t > 0 && t < n && t != dex {
                    evolves[dex] = true;
                    if pre[t].is_none() {
                        pre[t] = Some(dex as u32);
                    }
                }
            }
        }
        let mut root = vec![0u32; n];
        for (dex, r) in root.iter_mut().enumerate().skip(1) {
            let mut x = dex as u32;
            let mut steps = 0;
            while let Some(p) = pre[x as usize] {
                x = p;
                steps += 1;
                if steps > 4 {
                    break;
                }
            }
            *r = x;
        }
        Families { pre, root, evolves }
    }
}

fn bst(t: &Tables, dex: u32) -> u32 {
    t.personal[dex as usize].stats.iter().map(|x| *x as u32).sum()
}

/// A random species from `pool`, near `orig`'s base stat total when asked.
fn pick_species(t: &Tables, pool: &[u32], orig: u32, similar: bool, rng: &mut Rng) -> u32 {
    if pool.is_empty() {
        return orig;
    }
    if similar && orig > 0 {
        let b = bst(t, orig);
        let mut pct = 10u32;
        while pct <= 100 {
            let lo = b * (100 - pct.min(100)) / 100;
            let hi = b * (100 + pct) / 100;
            let near: Vec<u32> = pool.iter().copied().filter(|&d| (lo..=hi).contains(&bst(t, d))).collect();
            if near.len() >= 5 {
                return *rng.pick(&near);
            }
            pct += 10;
        }
    }
    *rng.pick(pool)
}

fn has_type(t: &Tables, dex: u32, ty: u8) -> bool {
    t.personal[dex as usize].types.contains(&ty)
}

/// What the randomizers changed in one player's game, for the world file.
#[derive(Clone, Debug, Default)]
pub struct Starters(pub Vec<u32>);

pub struct Ctx<'a> {
    pub m: &'a Model,
    pub o: &'a Opts,
    pub fam: &'a Families,
    pub legendary: &'a [u32],
}

impl Ctx<'_> {
    fn on(&self, opt: &str) -> bool {
        self.o.has(opt, "randomize") || self.o.has(opt, "shuffle")
    }
    fn is_legendary(&self, dex: u32) -> bool {
        self.legendary.contains(&dex)
    }
    fn all_species(&self) -> Vec<u32> {
        (1..=LAST_DEX).collect()
    }
}

/// Run every randomizer the options ask for, each on its own stream.
/// Order matters for content (types before trainers), not for randomness.
pub fn run(c: &Ctx, t: &mut Tables, stream: &dyn Fn(&str) -> Rng) -> Starters {
    base_stats(c, t, &mut stream("base_stats"));
    types(c, t, &mut stream("types"));
    catch_rates(c, t, &mut stream("catch_rates"));
    gender_ratios(c, t, &mut stream("gender_ratio"));
    abilities(c, t, &mut stream("abilities"));
    held_items(c, t, &mut stream("held_items"));
    evolutions(c, t, &mut stream("evolutions"));
    replace_evo_methods(c, t);
    tm_hm(c, t, &mut stream("tm_hm"));
    levelup_moves(c, t, &mut stream("levelup"));
    move_data(c, t, &mut stream("move_data"));
    type_chart(c, t, &mut stream("type_chart"));
    wild(c, t, &mut stream("wild"));
    statics(c, t, &mut stream("statics"));
    trades(c, t, &mut stream("trades"));
    let starters = starters(c, t, &mut stream("starters"));
    trainers(c, t, &starters, &mut stream("trainers"));
    Starters(starters)
}

// ------------------------------------------------------------ species data

fn base_stats(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_base_stats";
    if !c.on(opt) {
        return;
    }
    let follow = c.o.has(opt, "follow_evolutions");
    let random_total = c.o.has(opt, "random_total");
    let mut perm_of_root: BTreeMap<u32, [usize; 6]> = BTreeMap::new();
    for rec in 1..t.personal.len() {
        let dex = if rec <= LAST_DEX as usize { rec as u32 } else { 0 };
        let mut perm = [0usize, 1, 2, 3, 4, 5];
        if follow && dex > 0 {
            perm = *perm_of_root.entry(c.fam.root[dex as usize]).or_insert_with(|| {
                let mut p = [0usize, 1, 2, 3, 4, 5];
                rng.shuffle(&mut p);
                p
            });
        } else {
            rng.shuffle(&mut perm);
        }
        let old = t.personal[rec].stats;
        let total: u32 = old.iter().map(|x| *x as u32).sum();
        let mut new = [0u8; 6];
        if random_total {
            // the same total, split at random (each stat 10 to 255)
            let mut w = [0u32; 6];
            for x in w.iter_mut() {
                *x = 1 + rng.below(100) as u32;
            }
            let ws: u32 = w.iter().sum();
            let spare = total.saturating_sub(60);
            for i in 0..6 {
                new[i] = (10 + spare * w[i] / ws).min(255) as u8;
            }
        } else {
            for i in 0..6 {
                new[i] = old[perm[i]];
            }
        }
        // Shedinja keeps its 1 HP
        if dex == 292 {
            new[0] = 1;
        }
        t.personal[rec].stats = new;
    }
}

fn types(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_types";
    if c.o.has(opt, "shuffle") {
        let mut perm: Vec<u8> = (0..TYPES).collect();
        rng.shuffle(&mut perm);
        for p in t.personal.iter_mut().skip(1) {
            p.types = [perm[p.types[0] as usize % 17], perm[p.types[1] as usize % 17]];
        }
    } else if c.o.has(opt, "randomize") {
        // one pair per family when following evolutions, else per species
        let follow = c.o.has(opt, "follow_evolutions");
        let mut by_root: BTreeMap<u32, [u8; 2]> = BTreeMap::new();
        for rec in 1..t.personal.len() {
            let dual = t.personal[rec].types[0] != t.personal[rec].types[1];
            let draw = |rng: &mut Rng| {
                let a = rng.below(TYPES as u64) as u8;
                let b = if dual { (a + 1 + rng.below(TYPES as u64 - 1) as u8) % TYPES } else { a };
                [a, b]
            };
            let pair = if follow && rec <= LAST_DEX as usize {
                *by_root.entry(c.fam.root[rec]).or_insert_with(|| draw(rng))
            } else {
                draw(rng)
            };
            t.personal[rec].types = pair;
        }
    }
}

fn catch_rates(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_catch_rates";
    if c.o.has(opt, "shuffle") {
        let mut v: Vec<u8> = t.personal.iter().skip(1).map(|p| p.catch_rate).collect();
        rng.shuffle(&mut v);
        for (p, x) in t.personal.iter_mut().skip(1).zip(v) {
            p.catch_rate = x;
        }
    } else if c.o.has(opt, "randomize") {
        for p in t.personal.iter_mut().skip(1) {
            p.catch_rate = (3 + rng.below(253)) as u8;
        }
    }
}

fn gender_ratios(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_gender_ratio";
    if !c.on(opt) {
        return;
    }
    // the fixed ratios (always male 0, always female 254, genderless 255) stay
    let ratios = [31u8, 63, 127, 191];
    for p in t.personal.iter_mut().skip(1) {
        if !matches!(p.gender, 0 | 254 | 255) {
            p.gender = *rng.pick(&ratios);
        }
    }
}

fn abilities(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_abilities";
    if !c.on(opt) {
        return;
    }
    // Wonder Guard (25) only where it was
    for p in t.personal.iter_mut().skip(1) {
        for a in p.abilities.iter_mut() {
            if *a != 0 && *a != 25 {
                let mut x = 1 + rng.below(LAST_ABILITY as u64) as u8;
                if x == 25 {
                    x = 26;
                }
                *a = x;
            }
        }
    }
}

fn held_items(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_held_items";
    if !c.on(opt) {
        return;
    }
    let mut v: Vec<[u16; 3]> = t.personal.iter().skip(1).map(|p| p.items).collect();
    rng.shuffle(&mut v);
    for (p, x) in t.personal.iter_mut().skip(1).zip(v) {
        p.items = x;
    }
}

fn evolutions(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_evolutions";
    if !c.on(opt) {
        return;
    }
    let follow_type = c.o.has(opt, "follow_type");
    let pool: Vec<u32> = c.all_species().into_iter().filter(|&d| !c.is_legendary(d)).collect();
    for rec in 1..t.evolutions.len() {
        let base = if rec <= LAST_DEX as usize { rec as u32 } else { continue };
        for i in 0..t.evolutions[rec].len() {
            let cands: Vec<u32> = pool
                .iter()
                .copied()
                .filter(|&d| d != base)
                .filter(|&d| !follow_type || t.personal[d as usize].types.iter().any(|ty| has_type(t, base, *ty)))
                .collect();
            let orig = t.evolutions[rec][i][2] as u32;
            let new = pick_species(t, &cands, orig, false, rng);
            t.evolutions[rec][i][2] = new as u16;
        }
    }
}

/// Trade and beauty evolutions made possible alone: trade becomes a level,
/// trade holding an item becomes level up holding it, Karrablast and
/// Shelmet evolve with each other in the party.
fn replace_evo_methods(c: &Ctx, t: &mut Tables) {
    if !c.o.has("replace_evo_methods", "trade") {
        return;
    }
    for rec in 1..t.evolutions.len() {
        for row in t.evolutions[rec].iter_mut() {
            match row[0] {
                5 => *row = [4, 37, row[2]],
                6 => *row = [19, row[1], row[2]],
                7 => {
                    // Karrablast (588) wants Shelmet (616) and the other way round
                    let partner = if rec == 588 { 616 } else { 588 };
                    *row = [22, partner, row[2]];
                }
                16 => *row = [4, 30, row[2]],
                _ => {}
            }
        }
    }
}

fn tm_hm(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_tm_hm_compatibility";
    let all_tms = c.o.has(opt, "all_tms");
    let all_hms = c.o.has(opt, "all_hms");
    let random = c.o.has(opt, "randomize");
    if !(all_tms || all_hms || random) {
        return;
    }
    let follow = c.o.has(opt, "follow_evolutions");
    let mut by_root: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for rec in 1..t.personal.len() {
        let mut bits: Vec<u8> = t.personal[rec].machines.clone();
        if random {
            let keep = bits.len();
            let draw = |rng: &mut Rng| {
                let mut all: Vec<u8> = (0..=100).collect();
                rng.shuffle(&mut all);
                let mut b: Vec<u8> = all.into_iter().take(keep).collect();
                b.sort();
                b
            };
            bits = if follow && rec <= LAST_DEX as usize {
                by_root.entry(c.fam.root[rec]).or_insert_with(|| draw(rng)).clone()
            } else {
                draw(rng)
            };
        }
        if all_tms {
            bits.extend(0..95u8);
        }
        if all_hms {
            bits.extend(95..=100u8);
        }
        bits.sort();
        bits.dedup();
        t.personal[rec].machines = bits;
    }
}

fn levelup_moves(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_levelup_movesets";
    if !c.on(opt) {
        return;
    }
    let match_types = c.o.has(opt, "match_types");
    let moves: Vec<u32> = (1..=LAST_MOVE).filter(|&x| x != STRUGGLE && t.moves_ok(x)).collect();
    for rec in 1..t.learnsets.len() {
        let types = t.personal[rec].types;
        let own: Vec<u32> = moves.iter().copied().filter(|&x| types.contains(&t.move_type(x))).collect();
        for e in t.learnsets[rec].iter_mut() {
            let pool = if match_types && !own.is_empty() && rng.below(2) == 0 { &own } else { &moves };
            e[0] = *rng.pick(pool) as u16;
        }
    }
}

fn move_data(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_move_data";
    let ty = c.o.has(opt, "type");
    let power = c.o.has(opt, "power");
    let acc = c.o.has(opt, "accuracy");
    let pp = c.o.has(opt, "pp");
    if !(ty || power || acc || pp) {
        return;
    }
    // the moves machines teach keep their data (HMs are field moves too)
    let keep: Vec<u32> = t.machine_moves.clone();
    for id in 1..t.moves.len() {
        if keep.contains(&(id as u32)) || id as u32 == STRUGGLE {
            continue;
        }
        let Some(mv) = t.moves[id].as_mut() else { continue };
        if ty {
            mv.kind = rng.below(TYPES as u64) as u8;
        }
        if power && mv.power > 1 {
            mv.power = (20 + 5 * rng.below(27)) as u8; // 20 to 150
        }
        if acc && mv.accuracy > 0 && mv.accuracy <= 100 {
            mv.accuracy = (70 + 5 * rng.below(7)) as u8; // 70 to 100
        }
        if pp {
            mv.pp = (5 * (1 + rng.below(8))) as u8; // 5 to 40
        }
    }
}

fn type_chart(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    if !c.o.has("randomize_type_chart", "shuffle") {
        return;
    }
    let mut cells: Vec<u8> = t.type_chart.iter().flatten().copied().collect();
    rng.shuffle(&mut cells);
    for (i, row) in t.type_chart.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            *x = cells[i * 17 + j];
        }
    }
}

// ------------------------------------------------------------ encounters

fn wild(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_wild_pokemon";
    if !c.on(opt) {
        return;
    }
    let similar = c.o.has(opt, "similar_stats");
    let themed = c.o.has(opt, "type_themed_areas");
    let ensure_all = c.o.has(opt, "ensure_all");
    let pool: Vec<u32> = c.all_species().into_iter().filter(|&d| !c.is_legendary(d)).collect();
    // every species once first, when asked (legendaries too, so the people
    // who ask to see one can be answered): a shuffled list to draw from
    let mut must: Vec<u32> = if ensure_all { c.all_species() } else { Vec::new() };
    rng.shuffle(&mut must);
    for file in 0..t.wild.len() {
        let methods: Vec<String> = t.wild[file].first().map(|b| b.slots.keys().cloned().collect()).unwrap_or_default();
        let theme = rng.below(TYPES as u64) as u8;
        for method in methods {
            // the same original species maps to the same new one in all seasons
            let mut map: BTreeMap<u16, u16> = BTreeMap::new();
            for b in 0..t.wild[file].len() {
                let slots = t.wild[file][b].slots.get(&method).cloned().unwrap_or_default();
                let mut out = slots.clone();
                for (k, s) in slots.iter().enumerate() {
                    if s[0] == 0 {
                        continue;
                    }
                    let new = match map.get(&s[0]) {
                        Some(x) => *x,
                        None => {
                            let x = if let Some(x) = must.pop() {
                                x
                            } else {
                                let cands: Vec<u32> = if themed {
                                    let v: Vec<u32> = pool.iter().copied().filter(|&d| has_type(t, d, theme)).collect();
                                    if v.is_empty() { pool.clone() } else { v }
                                } else {
                                    pool.clone()
                                };
                                pick_species(t, &cands, s[0] as u32, similar, rng)
                            };
                            map.insert(s[0], x as u16);
                            x as u16
                        }
                    };
                    out[k] = [new, 0, s[2], s[3]];
                }
                t.wild[file][b].slots.insert(method.clone(), out);
            }
        }
    }
}

fn statics(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let defs = &c.m.species.statics;
    let pool_all: Vec<u32> = c.all_species();
    let non_leg: Vec<u32> = pool_all.iter().copied().filter(|&d| !c.is_legendary(d)).collect();
    let legs: Vec<u32> = c.legendary.to_vec();
    let mut monkey: Option<u32> = None;
    let mut shared: std::collections::BTreeMap<u32, u32> = Default::default();
    for (i, s) in defs.iter().enumerate() {
        if s.fixed || s.kind == "starter" {
            continue;
        }
        let opt = match s.kind.as_str() {
            "legendary" => "randomize_legendary_pokemon",
            "gift" => "randomize_gift_pokemon",
            _ => "randomize_static_pokemon",
        };
        if !c.on(opt) {
            continue;
        }
        let pool = if s.kind == "legendary" && c.o.has(opt, "keep_legendary") {
            &legs
        } else if c.o.has(opt, "no_legendaries") {
            &non_leg
        } else {
            &pool_all
        };
        let orig = t.statics[i][0];
        let mut new = pick_species(t, pool, orig, c.o.has(opt, "similar_stats"), rng);
        // the three Dreamyard monkeys stay one species unless split
        if s.name.starts_with("Dreamyard Gift Encounter") && !c.o.has(opt, "split_monkeys") {
            new = *monkey.get_or_insert(new);
        }
        // the Route 6 and Route 10 "item" encounters share one script per
        // original species (member 897), so they share the new species too
        if s.name.starts_with("Route 6 Item Encounter") || s.name.starts_with("Route 10 Item Encounter") {
            new = *shared.entry(orig).or_insert(new);
        }
        t.statics[i] = [new, 0];
    }
}

fn trades(c: &Ctx, t: &mut Tables, rng: &mut Rng) {
    let opt = "randomize_trade_pokemon";
    let pool: Vec<u32> = c.all_species().into_iter().filter(|&d| !c.is_legendary(d)).collect();
    for i in 0..t.trades.len() {
        if c.o.has(opt, "randomize_offer") {
            t.trades[i].0 = [*rng.pick(&pool), 0];
        }
        if c.o.has(opt, "randomize_request") {
            t.trades[i].1 = *rng.pick(&pool);
        }
    }
}

/// The three starters (the "starter" statics, in order left, middle, right).
fn starters(c: &Ctx, t: &mut Tables, rng: &mut Rng) -> Vec<u32> {
    let opt = "randomize_starter_pokemon";
    let idx: Vec<usize> =
        c.m.species.statics.iter().enumerate().filter(|(_, s)| s.kind == "starter").map(|(i, _)| i).collect();
    let pool_opt: Vec<u32> = c
        .o
        .0
        .get("starter_pool")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_u64()).map(|x| x as u32).filter(|&d| (1..=LAST_DEX).contains(&d)).collect())
        .unwrap_or_default();
    let random = c.on(opt) || pool_opt.len() >= 3;
    if !random {
        return idx.iter().map(|&i| t.statics[i][0]).collect();
    }
    let mut pool: Vec<u32> = if pool_opt.len() >= 3 {
        pool_opt
    } else if c.o.has(opt, "any_base") {
        c.all_species().into_iter().filter(|&d| c.fam.pre[d as usize].is_none() && !c.is_legendary(d)).collect()
    } else {
        // first stages that evolve twice, like the game's own starters
        c.all_species()
            .into_iter()
            .filter(|&d| {
                c.fam.pre[d as usize].is_none()
                    && t.evolutions[d as usize].iter().any(|r| c.fam.evolves[r[2] as usize])
            })
            .collect()
    };
    pool.sort();
    pool.dedup();
    let mut out = Vec::new();
    let mut used_types: Vec<u8> = Vec::new();
    for &i in &idx {
        let orig = t.statics[i][0];
        let mut cands: Vec<u32> = pool.iter().copied().filter(|d| !out.contains(d)).collect();
        if c.o.has(opt, "type_variety") {
            let v: Vec<u32> = cands
                .iter()
                .copied()
                .filter(|&d| !t.personal[d as usize].types.iter().any(|ty| used_types.contains(ty)))
                .collect();
            if !v.is_empty() {
                cands = v;
            }
        }
        let x = pick_species(t, &cands, orig, c.o.has(opt, "similar_stats"), rng);
        used_types.extend(t.personal[x as usize].types);
        out.push(x);
        t.statics[i] = [x, 0];
    }
    out
}

/// The moves a species knows at a level: its last four level-up moves.
fn moves_at(t: &Tables, dex: u32, level: u16) -> [u16; 4] {
    let mut known: Vec<u16> = Vec::new();
    for e in &t.learnsets[dex as usize] {
        if e[1] <= level && !known.contains(&e[0]) {
            known.push(e[0]);
        }
    }
    let mut out = [0u16; 4];
    for (i, mv) in known.iter().rev().take(4).enumerate() {
        out[i] = *mv;
    }
    out
}

fn trainers(c: &Ctx, t: &mut Tables, starters: &[u32], rng: &mut Rng) {
    let opt = "randomize_trainer_pokemon";
    if !c.on(opt) {
        return;
    }
    let similar = c.o.has(opt, "similar_stats");
    let themed = c.o.has(opt, "type_themed");
    let gym_types = c.o.has(opt, "shuffle_gym_types");
    let keep_starter = c.o.has(opt, "rivals_keep_starter");
    let pool: Vec<u32> = c.all_species().into_iter().filter(|&d| !c.is_legendary(d)).collect();
    // a type per gym town when shuffling gym types
    let mut town_type: BTreeMap<String, u8> = BTreeMap::new();
    let defs: BTreeMap<u32, &crate::species::TrainerDef> = c.m.species.trainers.iter().map(|d| (d.id, d)).collect();
    let vanilla_starters: Vec<u32> =
        c.m.species.statics.iter().filter(|s| s.kind == "starter").map(|s| s.vanilla[0][0]).collect();
    for id in 1..t.trainers.len() {
        let Some(mut tr) = t.trainers[id].take() else { continue };
        let def = defs.get(&(id as u32));
        let theme: Option<u8> = match (def.and_then(|d| d.gym.as_ref()), gym_types, themed) {
            (Some(town), true, _) => Some(*town_type.entry(town.clone()).or_insert_with(|| rng.below(TYPES as u64) as u8)),
            (_, _, true) => Some(rng.below(TYPES as u64) as u8),
            _ => None,
        };
        for mon in tr.party.iter_mut() {
            // a starter of the original game (a rival's) becomes the new
            // starter in the same place, evolved to the trainer's level
            let vanilla_idx = vanilla_starters.iter().position(|&s| c.fam.root[mon.species as usize] == s);
            let new = if let (true, Some(i), 3) = (keep_starter, vanilla_idx, starters.len()) {
                evolve_to_level(t, starters[i], mon.level)
            } else {
                let cands: Vec<u32> = match theme {
                    Some(ty) => {
                        let v: Vec<u32> = pool.iter().copied().filter(|&d| has_type(t, d, ty)).collect();
                        if v.is_empty() { pool.clone() } else { v }
                    }
                    None => pool.clone(),
                };
                pick_species(t, &cands, mon.species as u32, similar, rng)
            };
            mon.species = new as u16;
            mon.form = 0;
            if mon.moves.is_some() {
                mon.moves = Some(moves_at(t, new, mon.level));
            }
        }
        t.trainers[id] = Some(tr);
    }
}

/// Follow level evolutions of a species up to a level.
fn evolve_to_level(t: &Tables, dex: u32, level: u16) -> u32 {
    let mut d = dex;
    for _ in 0..3 {
        let next = t.evolutions[d as usize].iter().find(|r| r[0] == 4 && r[1] <= level).map(|r| r[2] as u32);
        match next {
            Some(n) if n > 0 && n <= LAST_DEX => d = n,
            _ => break,
        }
    }
    d
}
