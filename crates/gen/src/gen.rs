//! Generation: options, rule evaluation, the reachability sweep over all
//! players, the item fill, and the world.
//!
//! Determinism: our own RNG; only Vec and BTreeMap where order matters; no
//! floats; ranges in u64. Same configuration and seed give the same world.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::data::Rule;
use crate::model::Model;
use crate::rng::{fnv1a, Rng};
use crate::species::{self, Picks, PlayerSpecies, SpState, Tables, Use};

pub const GENERATOR_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const SCHEMA: u32 = 1;

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct PlayerConfig {
    pub name: String,
    /// "black" or "white"
    pub version: String,
    #[serde(default)]
    pub options: Map<String, Value>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Config {
    pub schema: u32,
    pub seed: u64,
    #[serde(default)]
    pub options: Map<String, Value>,
    pub players: Vec<PlayerConfig>,
}

/// v1 defaults; a player's options override the game's, which override these.
fn defaults() -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("goal".into(), Value::String("ghetsis".into()));
    m.insert("season_control".into(), Value::String("vanilla".into()));
    m.insert("modify_logic".into(), Value::Array(vec![]));
    m.insert("randomize_wild_pokemon".into(), Value::Array(vec![]));
    m.insert("starter_pool".into(), Value::Array(vec![]));
    // where the logic finds species besides wild grass and water
    m.insert(
        "species_sources".into(),
        Value::Array(["static", "evolutions", "form_change", "trainers"].iter().map(|s| Value::String((*s).into())).collect()),
    );
    m
}

pub struct Opts(pub Map<String, Value>);

impl Opts {
    fn str(&self, k: &str) -> &str {
        self.0.get(k).and_then(|v| v.as_str()).unwrap_or("")
    }
    fn has(&self, k: &str, v: &str) -> bool {
        match self.0.get(k) {
            Some(Value::Array(a)) => a.iter().any(|x| x.as_str() == Some(v)),
            Some(Value::String(s)) => s == v,
            _ => false,
        }
    }
}

pub(crate) struct Ctx<'a> {
    pub m: &'a Model,
    pub opts: &'a Opts,
    pub counts: &'a [u32],
    pub reach: &'a [bool],
    pub sp: &'a SpState,
    pub ps: &'a PlayerSpecies,
}

pub(crate) fn eval(r: &Rule, c: &Ctx) -> bool {
    match r {
        Rule::Const(b) => *b,
        Rule::Has(i, n) => c.counts[*i as usize] >= *n,
        Rule::All(v) => v.iter().all(|x| eval(x, c)),
        Rule::Any(v) => v.iter().any(|x| eval(x, c)),
        Rule::Not(x) => !eval(x, c),
        Rule::Reach(g) => c.reach[*g as usize],
        Rule::Named(i) => eval(&c.m.rules[*i as usize], c),
        Rule::If(cond, then) => !eval(cond, c) || eval(then, c),
        Rule::OptEq(o, v) => c.opts.0.get(o) == Some(v),
        Rule::OptHas(o, v) => c.opts.has(o, v),
        Rule::OptSet(o) => matches!(c.opts.0.get(o), Some(Value::Bool(true))),
        Rule::Value(name) => name == "monkeys_not_split" && c.ps.monkeys_not_split,
        Rule::SetAny(s) => c.sp.set_hit[*s as usize],
        Rule::Seen(n) => c.sp.seen_n >= *n,
        Rule::Caught(mon) => c.sp.caught[*mon as usize],
        // levels count in steps of 5, as BWAP's level events do
        Rule::Level(l) => c.sp.level / 5 >= *l / 5,
        Rule::Pick(0) => c.ps.other_mon.map(|x| c.sp.caught[x as usize]) == Some(true),
        Rule::Pick(_) => c.ps.tm_item.map(|x| c.counts[x as usize] > 0) == Some(true),
    }
}

/// One player's view of the logic: what is in their world.
pub struct PlayerWorld {
    pub name: String,
    pub version: String,
    pub opts: Opts,
    pub region_on: Vec<bool>,
    pub conn_on: Vec<bool>,
    pub loc_on: Vec<bool>,
    pub event_on: Vec<bool>,
    pub goal: u32,
    pub tables: Tables,
    pub ps: PlayerSpecies,
}

/// The state of a sweep: per player, item counts, reachable regions, which
/// locations and events were collected, and the species state.
#[derive(Clone)]
pub struct State {
    pub counts: Vec<Vec<u32>>,
    pub reach: Vec<Vec<bool>>,
    pub got_loc: Vec<Vec<bool>>,
    pub got_event: Vec<Vec<bool>>,
    pub sp: Vec<SpState>,
}

impl State {
    fn new(m: &Model, ws: &[PlayerWorld]) -> State {
        let players = ws.len();
        State {
            counts: vec![vec![0; m.items.len()]; players],
            reach: vec![vec![false; m.regions.len()]; players],
            got_loc: vec![vec![false; m.locs.len()]; players],
            got_event: vec![vec![false; m.events.len()]; players],
            sp: ws
                .iter()
                .map(|w| {
                    let mut s = SpState::new(m);
                    s.src_done = vec![false; w.ps.sources.len()];
                    s.evo_done = vec![false; w.ps.evos.len()];
                    s
                })
                .collect(),
        }
    }
}

fn ctx<'a>(m: &'a Model, w: &'a PlayerWorld, st: &'a State, p: usize) -> Ctx<'a> {
    Ctx { m, opts: &w.opts, counts: &st.counts[p], reach: &st.reach[p], sp: &st.sp[p], ps: &w.ps }
}

/// Catch and see what the player's reached places give, until nothing changes.
fn species_step(m: &Model, w: &PlayerWorld, st: &mut State, p: usize) -> bool {
    let ps = &w.ps;
    let mut changed = false;
    loop {
        let mut grew = false;
        if let Some(h) = ps.hall {
            if st.reach[p][h as usize] && st.sp[p].level < 100 {
                st.sp[p].level = 100;
                grew = true;
            }
        }
        for (k, s) in ps.sources.iter().enumerate() {
            if !s.on || st.sp[p].src_done[k] || !s.regions.iter().any(|r| st.reach[p][*r as usize]) {
                continue;
            }
            if !eval(&s.rule, &ctx(m, w, st, p)) {
                continue;
            }
            let sp = &mut st.sp[p];
            sp.src_done[k] = true;
            sp.level = sp.level.max(s.level);
            for &mon in &s.mons {
                if s.seen {
                    sp.see(m.facts.mons[mon as usize].0 as u32);
                } else {
                    sp.catch(m, ps, mon);
                }
            }
            grew = true;
        }
        for (k, e) in ps.evos.iter().enumerate() {
            if st.sp[p].evo_done[k] || !st.sp[p].caught[e.base as usize] {
                continue;
            }
            if st.sp[p].caught[e.target as usize] {
                st.sp[p].evo_done[k] = true;
                continue;
            }
            if eval(&e.rule, &ctx(m, w, st, p)) {
                st.sp[p].evo_done[k] = true;
                st.sp[p].catch(m, ps, e.target);
                grew = true;
            }
        }
        for g in &ps.forms {
            if !st.reach[p][g.region as usize] || !g.mons.iter().any(|x| st.sp[p].caught[*x as usize]) {
                continue;
            }
            for (i, &mon) in g.mons.iter().enumerate() {
                if st.sp[p].caught[mon as usize] {
                    continue;
                }
                let ok = g.rules.get(i).map(|r| eval(r, &ctx(m, w, st, p))).unwrap_or(false);
                if ok {
                    st.sp[p].catch(m, ps, mon);
                    grew = true;
                }
            }
        }
        if !grew {
            return changed;
        }
        changed = true;
    }
}

/// placement[player][location] = Some((receiver, item))
pub type Placement = Vec<Vec<Option<(u32, u32)>>>;

fn loc_ok(m: &Model, w: &PlayerWorld, st: &State, p: usize, l: usize) -> bool {
    let loc = &m.locs[l];
    w.loc_on[l]
        && st.reach[p][loc.region as usize]
        && eval(&loc.rule, &ctx(m, w, st, p))
}

/// Grow one player's reach, events and species until nothing changes.
/// Returns whether anything grew.
fn grow_player(m: &Model, w: &PlayerWorld, st: &mut State, p: usize) -> bool {
    let mut any = false;
    if !st.reach[p][m.menu as usize] {
        st.reach[p][m.menu as usize] = true;
        any = true;
    }
    loop {
        let mut grew = false;
        // regions: repeat a pass until stable (rules may ask for reach)
        loop {
            let mut more = false;
            for (ci, cn) in m.conns.iter().enumerate() {
                if !w.conn_on[ci] {
                    continue;
                }
                let (a, b) = (cn.from as usize, cn.to as usize);
                let fwd = st.reach[p][a] && !st.reach[p][b] && w.region_on[b];
                let back = !cn.one_way && st.reach[p][b] && !st.reach[p][a] && w.region_on[a];
                if fwd && eval(&cn.rule, &ctx(m, w, st, p)) {
                    st.reach[p][b] = true;
                    more = true;
                } else if back && eval(&cn.back, &ctx(m, w, st, p)) {
                    st.reach[p][a] = true;
                    more = true;
                }
            }
            if !more {
                break;
            }
            grew = true;
        }
        for (ei, ev) in m.events.iter().enumerate() {
            if st.got_event[p][ei] || !w.event_on[ei] || !st.reach[p][ev.region as usize] {
                continue;
            }
            if eval(&ev.rule, &ctx(m, w, st, p)) {
                st.got_event[p][ei] = true;
                st.counts[p][ev.item as usize] += 1;
                grew = true;
            }
        }
        if species_step(m, w, st, p) {
            grew = true;
        }
        if !grew {
            return any;
        }
        any = true;
    }
}

/// Expand everything reachable, collecting events and placed items, until
/// nothing changes. A player is looked at again only when something of
/// theirs changed (an item arrived), so large worlds stay fast.
fn sweep(m: &Model, ws: &[PlayerWorld], st: &mut State, placed: &Placement) {
    let n = ws.len();
    let mut dirty = vec![true; n];
    let mut first = true;
    loop {
        let mut touched = vec![false; n];
        for (p, w) in ws.iter().enumerate() {
            if dirty[p] {
                // an item arrived (or the first round): its locations are
                // looked at again even when nothing else grew
                dirty[p] = false;
                grow_player(m, w, st, p);
                touched[p] = true;
            }
        }
        let mut any = false;
        for p in 0..n {
            if !(first || touched[p]) {
                continue;
            }
            for l in 0..m.locs.len() {
                if st.got_loc[p][l] {
                    continue;
                }
                if let Some((rcv, item)) = placed[p][l] {
                    if loc_ok(m, &ws[p], st, p, l) {
                        st.got_loc[p][l] = true;
                        st.counts[rcv as usize][item as usize] += 1;
                        dirty[rcv as usize] = true;
                        any = true;
                    }
                }
            }
        }
        first = false;
        if !any && !dirty.iter().any(|d| *d) {
            return;
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlayerOut {
    pub slot: u32,
    pub name: String,
    pub version: String,
    pub options: Map<String, Value>,
    /// [location id, item id, receiving slot], by location id
    pub locations: Vec<[u32; 3]>,
    /// locations of this game no item can ever unlock (none with the
    /// default options)
    pub left_out: Vec<u32>,
    pub starters: Vec<u32>,
    /// the per-world picks some locations ask for
    pub picks: Picks,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct World {
    pub format: String,
    pub schema: u32,
    pub generator: String,
    pub data_hash: String,
    pub seed: u64,
    pub config_hash: String,
    pub players: Vec<PlayerOut>,
}

fn effective(cfg: &Config, p: &PlayerConfig) -> Map<String, Value> {
    let mut m = defaults();
    for (k, v) in &cfg.options {
        m.insert(k.clone(), v.clone());
    }
    for (k, v) in &p.options {
        m.insert(k.clone(), v.clone());
    }
    m.insert("version".into(), Value::String(p.version.clone()));
    m
}

/// A random stream of its own for one stage of one player, so that a new
/// option or stage never reshuffles the others.
pub fn stream(cfg: &Config, stage: &str, player: usize) -> Rng {
    Rng::new(fnv1a(&[&config_seed(cfg).to_le_bytes(), stage.as_bytes(), &(player as u64).to_le_bytes()]))
}

fn build_player(m: &Model, cfg: &Config, p: &PlayerConfig, index: usize) -> Result<PlayerWorld, String> {
    if p.version != "black" && p.version != "white" {
        return Err(format!("{}: version must be black or white", p.name));
    }
    let opts = Opts(effective(cfg, p));
    let tables = Tables::vanilla(m, if p.version == "black" { 0 } else { 1 });
    let flags = PlayerSpecies::flags(m, &tables);
    let empty_counts = vec![0u32; m.items.len()];
    let empty_reach = vec![false; m.regions.len()];
    let empty_sp = SpState::new(m);
    let ctx = Ctx { m, opts: &opts, counts: &empty_counts, reach: &empty_reach, sp: &empty_sp, ps: &flags };
    let region_on: Vec<bool> = m.region_include.iter().map(|r| eval(r, &ctx)).collect();
    let conn_on: Vec<bool> = m
        .conns
        .iter()
        .map(|c| eval(&c.include, &ctx) && region_on[c.from as usize] && region_on[c.to as usize])
        .collect();
    let loc_on: Vec<bool> = m.locs.iter().map(|l| eval(&l.include, &ctx) && region_on[l.region as usize]).collect();
    let event_on: Vec<bool> = m.events.iter().map(|e| eval(&e.include, &ctx) && region_on[e.region as usize]).collect();
    let goal_name = match opts.str("goal") {
        "ghetsis" => "[Event] Defeating Ghetsis",
        "champion" => "[Event] Defeating champ Alder",
        g => return Err(format!("{}: unknown goal {g}", p.name)),
    };
    let goal = m.item_by_name[goal_name];
    let use_ = Use {
        statics: opts.has("species_sources", "static"),
        trades: opts.has("species_sources", "trades"),
        evolutions: opts.has("species_sources", "evolutions"),
        form_change: opts.has("species_sources", "form_change"),
        trainers: opts.has("species_sources", "trainers"),
    };
    let mut rng = stream(cfg, "picks", index);
    let ps = species::build(m, &tables, &use_, &|r: &Rule| eval(r, &ctx), &mut rng)?;
    Ok(PlayerWorld {
        name: p.name.clone(),
        version: p.version.clone(),
        opts,
        region_on,
        conn_on,
        loc_on,
        event_on,
        goal,
        tables,
        ps,
    })
}

/// The item's class for this player (two classes depend on options).
fn class_of<'a>(m: &'a Model, w: &PlayerWorld, i: usize) -> &'a str {
    let it = &m.items[i];
    // the TM the Driftveil lady asks about unlocks her location
    if w.ps.tm_item == Some(i as u32) {
        return "progression";
    }
    match it.class.as_str() {
        "tm_hm_class" => {
            if w.opts.has("goal", "tmhm_hunt") {
                "progression"
            } else {
                "useful"
            }
        }
        "dowsing_machine_logic" => {
            if w.opts.has("modify_logic", "require_dowsing") {
                "progression"
            } else {
                "useful"
            }
        }
        c => c,
    }
}

fn pool_items(m: &Model, w: &PlayerWorld) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let mut prog = Vec::new();
    let mut useful = Vec::new();
    let mut filler = Vec::new();
    for (i, it) in m.items.iter().enumerate() {
        if it.id.is_none() {
            continue;
        }
        if it.group == "season" && w.opts.str("season_control") == "vanilla" {
            continue;
        }
        if matches!(it.table.as_str(), "unused" | "useless" | "mail") {
            continue;
        }
        match class_of(m, w, i) {
            "progression" => prog.push(i as u32),
            "useful" => useful.push(i as u32),
            "filler" => filler.push(i as u32),
            _ => {}
        }
    }
    (prog, useful, filler)
}

pub fn config_seed(cfg: &Config) -> u64 {
    let canon = serde_json::to_string(cfg).expect("config serializes");
    fnv1a(&[&cfg.seed.to_le_bytes(), canon.as_bytes()])
}

pub fn generate(m: &Model, cfg: &Config) -> Result<World, String> {
    if cfg.schema != SCHEMA {
        return Err(format!("configuration schema {} (this generator reads {SCHEMA})", cfg.schema));
    }
    if cfg.players.is_empty() || cfg.players.len() > 16 {
        return Err("1 to 16 players".into());
    }
    let mut names = std::collections::BTreeSet::new();
    for p in &cfg.players {
        if !names.insert(p.name.clone()) {
            return Err(format!("two players named {}", p.name));
        }
    }
    let ws: Vec<PlayerWorld> =
        cfg.players.iter().enumerate().map(|(i, p)| build_player(m, cfg, p, i)).collect::<Result<_, _>>()?;
    let np = ws.len();
    let mut rng = Rng::new(config_seed(cfg));

    // progression items of every player
    let mut prog_all: Vec<(u32, u32)> = Vec::new();
    for (p, w) in ws.iter().enumerate() {
        let (prog, _, _) = pool_items(m, w);
        prog_all.extend(prog.iter().map(|&i| (p as u32, i)));
    }

    // which locations can ever be reached (all progression assumed): the rest
    // is left out of the world
    let mut full = State::new(m, &ws);
    for &(p, i) in &prog_all {
        full.counts[p as usize][i as usize] += 1;
    }
    let none: Placement = vec![vec![None; m.locs.len()]; np];
    sweep(m, &ws, &mut full, &none);
    let mut in_world: Vec<Vec<bool>> = vec![vec![false; m.locs.len()]; np];
    for p in 0..np {
        for l in 0..m.locs.len() {
            in_world[p][l] = loc_ok(m, &ws[p], &full, p, l);
        }
        if full.counts[p][ws[p].goal as usize] == 0 {
            return Err(format!("{}: the goal cannot be reached even with every item", ws[p].name));
        }
    }

    // each player's pool fills exactly their own number of locations
    let mut fill_rest: Vec<(u32, u32)> = Vec::new();
    for (p, w) in ws.iter().enumerate() {
        let n_loc = in_world[p].iter().filter(|x| **x).count();
        let n_prog = prog_all.iter().filter(|(q, _)| *q as usize == p).count();
        if n_prog > n_loc {
            return Err(format!("{}: {n_prog} progression items for {n_loc} locations", w.name));
        }
        let (_, useful, filler) = pool_items(m, w);
        let mut rest: Vec<u32> = useful.into_iter().take(n_loc - n_prog).collect();
        while rest.len() < n_loc - n_prog {
            rest.push(*rng.pick(&filler));
        }
        fill_rest.extend(rest.into_iter().map(|i| (p as u32, i)));
    }

    let mut attempt = 0;
    let placed = loop {
        attempt += 1;
        match fill_progression(m, &ws, &in_world, &prog_all, &mut rng) {
            Ok(p) => break p,
            Err(e) if attempt >= 20 => return Err(format!("fill failed 20 times: {e}")),
            Err(_) => continue,
        }
    };
    let mut placed = placed;

    // the rest: useful and filler into the empty locations
    let mut empty: Vec<(u32, u32)> = Vec::new();
    for p in 0..np {
        for l in 0..m.locs.len() {
            if in_world[p][l] && placed[p][l].is_none() {
                empty.push((p as u32, l as u32));
            }
        }
    }
    if empty.len() != fill_rest.len() {
        return Err(format!("{} empty locations for {} items", empty.len(), fill_rest.len()));
    }
    rng.shuffle(&mut empty);
    rng.shuffle(&mut fill_rest);
    for ((p, l), (rcv, item)) in empty.into_iter().zip(fill_rest) {
        placed[p as usize][l as usize] = Some((rcv, item));
    }

    // the world must be finishable from nothing
    let mut st = State::new(m, &ws);
    sweep(m, &ws, &mut st, &placed);
    for (p, w) in ws.iter().enumerate() {
        if st.counts[p][w.goal as usize] == 0 {
            return Err(format!("{}: world not finishable", w.name));
        }
    }

    let players = ws
        .iter()
        .enumerate()
        .map(|(p, w)| {
            let mut locs: Vec<[u32; 3]> = (0..m.locs.len())
                .filter_map(|l| placed[p][l].map(|(r, i)| [m.locs[l].id, m.items[i as usize].id.unwrap(), r + 1]))
                .collect();
            locs.sort();
            let mut left_out: Vec<u32> =
                (0..m.locs.len()).filter(|&l| w.loc_on[l] && !in_world[p][l]).map(|l| m.locs[l].id).collect();
            left_out.sort();
            let pool: Vec<u32> = w
                .opts
                .0
                .get("starter_pool")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_u64()).map(|x| x as u32).collect())
                .unwrap_or_default();
            let starters = pick_starters(&pool, &mut rng);
            PlayerOut {
                slot: p as u32 + 1,
                name: w.name.clone(),
                version: w.version.clone(),
                options: w.opts.0.clone(),
                locations: locs,
                left_out,
                starters,
                picks: w.ps.picks.clone(),
            }
        })
        .collect();
    Ok(World {
        format: "arkhipelagos-world".into(),
        schema: SCHEMA,
        generator: GENERATOR_VERSION.into(),
        data_hash: format!("{:016x}", crate::data::data_hash()),
        seed: cfg.seed,
        config_hash: format!("{:016x}", config_seed(cfg)),
        players,
    })
}

/// Starters from a list the configuration gives (species numbers). v1 needs
/// no game tables for this; an empty list keeps the game's own starters.
fn pick_starters(pool: &[u32], rng: &mut Rng) -> Vec<u32> {
    let mut p: Vec<u32> = pool.to_vec();
    p.sort();
    p.dedup();
    if p.len() < 3 {
        return Vec::new();
    }
    rng.shuffle(&mut p);
    p.truncate(3);
    p
}

/// Assumed fill: take progression items in random order; for each, assume
/// every item not yet placed is held, sweep, and put it in a random empty
/// location that is reachable then.
fn fill_progression(
    m: &Model,
    ws: &[PlayerWorld],
    in_world: &[Vec<bool>],
    prog_all: &[(u32, u32)],
    rng: &mut Rng,
) -> Result<Placement, String> {
    let np = ws.len();
    let mut items: Vec<(u32, u32)> = prog_all.to_vec();
    items.sort();
    rng.shuffle(&mut items);
    let mut placed: Placement = vec![vec![None; m.locs.len()]; np];
    let mut base = State::new(m, ws);
    for &(p, i) in &items {
        base.counts[p as usize][i as usize] += 1;
    }
    while let Some((p, i)) = items.pop() {
        base.counts[p as usize][i as usize] -= 1;
        let mut st = base.clone();
        sweep(m, ws, &mut st, &placed);
        let mut cands: Vec<(u32, u32)> = Vec::new();
        for q in 0..np {
            for l in 0..m.locs.len() {
                if in_world[q][l] && placed[q][l].is_none() && loc_ok(m, &ws[q], &st, q, l) {
                    cands.push((q as u32, l as u32));
                }
            }
        }
        if cands.is_empty() {
            return Err(format!("no place for {}", m.items[i as usize].name));
        }
        let (q, l) = *rng.pick(&cands);
        placed[q as usize][l as usize] = Some((p, i));
    }
    Ok(placed)
}

/// The playthrough in spheres for one generated world: sphere 0 is what
/// player 1 reaches with nothing; each next sphere adds the items found in
/// the previous one. Returns, per sphere, how many locations of all players
/// are reachable. A logic that holds back nothing would show one sphere.
pub fn spheres(m: &Model, cfg: &Config, w: &World) -> Result<Vec<usize>, String> {
    let ws: Vec<PlayerWorld> =
        cfg.players.iter().enumerate().map(|(i, p)| build_player(m, cfg, p, i)).collect::<Result<_, _>>()?;
    let np = ws.len();
    let by_id: BTreeMap<u32, usize> = m.locs.iter().enumerate().map(|(i, l)| (l.id, i)).collect();
    let item_by_id: BTreeMap<u32, usize> =
        m.items.iter().enumerate().filter_map(|(i, it)| it.id.map(|id| (id, i))).collect();
    let mut placed: Placement = vec![vec![None; m.locs.len()]; np];
    for (p, po) in w.players.iter().enumerate() {
        for [loc, item, rcv] in &po.locations {
            placed[p][by_id[loc]] = Some((rcv - 1, item_by_id[item] as u32));
        }
    }
    // sphere by sphere: collect only what was reachable at the start of the round
    let mut st = State::new(m, &ws);
    let mut out = Vec::new();
    let none: Placement = vec![vec![None; m.locs.len()]; np];
    loop {
        sweep(m, &ws, &mut st, &none); // regions and events only
        let mut found = Vec::new();
        let mut reachable = 0;
        for p in 0..np {
            for l in 0..m.locs.len() {
                if placed[p][l].is_some() && loc_ok(m, &ws[p], &st, p, l) {
                    reachable += 1;
                    if !st.got_loc[p][l] {
                        found.push((p, l));
                    }
                }
            }
        }
        out.push(reachable);
        if found.is_empty() {
            break;
        }
        for (p, l) in found {
            st.got_loc[p][l] = true;
            let (r, i) = placed[p][l].unwrap();
            st.counts[r as usize][i as usize] += 1;
        }
    }
    Ok(out)
}

/// A summary for reports.
pub fn summary(w: &World) -> BTreeMap<String, usize> {
    let mut s = BTreeMap::new();
    for p in &w.players {
        s.insert(format!("{} locations", p.name), p.locations.len());
        s.insert(format!("{} left out", p.name), p.left_out.len());
    }
    s
}
