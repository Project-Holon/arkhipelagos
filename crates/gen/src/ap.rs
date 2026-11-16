//! The Archipelago world's data (the apworld, `apworld/arkhipelagos`).
//!
//! Python never computes names, ids, pools, tables or logic of its own.
//! It reads:
//! - the static part (`static_json`): every item and location with its
//!   permanent number, written into the apworld at build time;
//! - one player's pack (`pack_json`): that player's game (tables made by
//!   the randomizers, regions, connections, locations, story events,
//!   species sources) with every rule folded for the player's options, the
//!   item pool and the data the mod needs (`slot`).
//!
//! `reach_json` answers which locations one player can reach with a given
//! set of items; tests compare it with the Python logic.

use serde_json::{json, Map, Value};

use super::{build_player, eval, loc_ok, pool_items, sweep, Config, Ctx, PlayerWorld, Placement, State};
use crate::data::Rule;
use crate::model::Model;
use crate::species::{Changes, SpState};

/// The format of the static file and the packs.
pub const FORMAT: &str = "arkhipelagos-ap/1";

/// A rule as JSON for Python: `true`/`false`, `["has", item, n]`,
/// `["all", r...]`, `["any", r...]`, `["not", r]`, `["reach", region]`,
/// `["rule", i]`, `["if", c, t]`, `["set", s]`, `["seen", n]`,
/// `["caught", mon]`, `["level", l]`.
fn rule_json(m: &Model, r: &Rule) -> Value {
    match r {
        Rule::Const(b) => json!(b),
        Rule::Has(i, n) => json!(["has", m.items[*i as usize].name, n]),
        Rule::All(v) => {
            let mut a = vec![json!("all")];
            a.extend(v.iter().map(|x| rule_json(m, x)));
            Value::Array(a)
        }
        Rule::Any(v) => {
            let mut a = vec![json!("any")];
            a.extend(v.iter().map(|x| rule_json(m, x)));
            Value::Array(a)
        }
        Rule::Not(x) => json!(["not", rule_json(m, x)]),
        Rule::Reach(g) => json!(["reach", m.regions[*g as usize]]),
        Rule::Named(i) => json!(["rule", i]),
        Rule::If(c, t) => json!(["if", rule_json(m, c), rule_json(m, t)]),
        Rule::SetAny(s) => json!(["set", s]),
        Rule::Seen(n) => json!(["seen", n]),
        Rule::Caught(mon) => json!(["caught", mon]),
        Rule::Level(l) => json!(["level", l]),
        // folded away before export
        Rule::OptEq(..) | Rule::OptHas(..) | Rule::OptSet(..) | Rule::Value(_) | Rule::Pick(_) => {
            unreachable!("option rules are folded")
        }
    }
}

/// Fold what is fixed for this player (options, picks, per-world values)
/// into constants, and simplify.
fn fold(r: &Rule, c: &Ctx) -> Rule {
    match r {
        Rule::OptEq(..) | Rule::OptHas(..) | Rule::OptSet(..) | Rule::Value(_) => Rule::Const(eval(r, c)),
        Rule::Pick(0) => c.ps.other_mon.map_or(Rule::Const(false), Rule::Caught),
        Rule::Pick(_) => c.ps.tm_item.map_or(Rule::Const(false), |i| Rule::Has(i, 1)),
        Rule::All(v) => {
            let mut out = Vec::new();
            for x in v {
                match fold(x, c) {
                    Rule::Const(true) => {}
                    Rule::Const(false) => return Rule::Const(false),
                    y => out.push(y),
                }
            }
            match out.len() {
                0 => Rule::Const(true),
                1 => out.pop().unwrap(),
                _ => Rule::All(out),
            }
        }
        Rule::Any(v) => {
            let mut out = Vec::new();
            for x in v {
                match fold(x, c) {
                    Rule::Const(false) => {}
                    Rule::Const(true) => return Rule::Const(true),
                    y => out.push(y),
                }
            }
            match out.len() {
                0 => Rule::Const(false),
                1 => out.pop().unwrap(),
                _ => Rule::Any(out),
            }
        }
        Rule::Not(x) => match fold(x, c) {
            Rule::Const(b) => Rule::Const(!b),
            y => Rule::Not(Box::new(y)),
        },
        Rule::If(a, t) => match fold(a, c) {
            Rule::Const(false) => Rule::Const(true),
            Rule::Const(true) => fold(t, c),
            y => match fold(t, c) {
                Rule::Const(true) => Rule::Const(true),
                Rule::Const(false) => Rule::Not(Box::new(y)),
                z => Rule::If(Box::new(y), Box::new(z)),
            },
        },
        x => x.clone(),
    }
}

/// Every item and location with its permanent number.
pub fn static_json(m: &Model) -> Value {
    let items: Vec<Value> = m
        .items
        .iter()
        .filter_map(|it| it.id.map(|id| json!({"name": it.name, "id": id, "group": it.group})))
        .collect();
    let locations: Vec<Value> = m
        .locs
        .iter()
        .map(|l| json!({"name": l.name, "id": l.id, "region": m.regions[l.region as usize]}))
        .collect();
    json!({
        "format": FORMAT,
        "generator": super::GENERATOR_VERSION,
        "data_hash": format!("{:016x}", crate::data::data_hash()),
        "items": items,
        "locations": locations,
    })
}

/// Locations of the player's game that some set of their own items opens
/// (with every progression item held): the rest is left out of the world.
fn in_world(m: &Model, w: &PlayerWorld) -> (Vec<bool>, State) {
    let ws = std::slice::from_ref(w);
    let mut st = State::new(m, ws);
    let (prog, _, _) = pool_items(m, w);
    for i in prog {
        st.counts[0][i as usize] += 1;
    }
    sweep(m, ws, &mut st, &vec![vec![None; m.locs.len()]; 1]);
    let inw = (0..m.locs.len()).map(|l| loc_ok(m, w, &st, 0, l)).collect();
    (inw, st)
}

/// One player's pack. `index` is the player's place in `cfg`.
pub fn pack_json(m: &Model, cfg: &Config, index: usize) -> Result<Value, String> {
    let p = cfg.players.get(index).ok_or("no such player")?;
    let w = build_player(m, cfg, p, index)?;
    let empty_counts = vec![0u32; m.items.len()];
    let empty_reach = vec![false; m.regions.len()];
    let empty_sp = SpState::new(m);
    let c = Ctx { m, opts: &w.opts, counts: &empty_counts, reach: &empty_reach, sp: &empty_sp, ps: &w.ps };
    let r = |x: &Rule| rule_json(m, &fold(x, &c));
    let on = |g: u32| w.region_on[g as usize];
    let regions: Vec<&String> = (0..m.regions.len()).filter(|g| w.region_on[*g]).map(|g| &m.regions[g]).collect();
    let rules: Vec<Value> = m.rules.iter().map(r).collect();
    let conns: Vec<Value> = m
        .conns
        .iter()
        .enumerate()
        .filter(|(i, _)| w.conn_on[*i])
        .map(|(_, cn)| {
            json!([m.regions[cn.from as usize], m.regions[cn.to as usize], cn.one_way, r(&cn.rule), r(&cn.back)])
        })
        .collect();
    let (inw, full) = in_world(m, &w);
    // only events that can happen (Archipelago wants every location
    // reachable with every item); the others never change the sweep
    let fc = Ctx { m, opts: &w.opts, counts: &full.counts[0], reach: &full.reach[0], sp: &full.sp[0], ps: &w.ps };
    let locations: Vec<Value> = (0..m.locs.len())
        .filter(|l| inw[*l])
        .map(|l| json!([m.locs[l].id, r(&m.locs[l].rule)]))
        .collect();
    let mut left_out: Vec<u32> = (0..m.locs.len()).filter(|&l| w.loc_on[l] && !inw[l]).map(|l| m.locs[l].id).collect();
    left_out.sort();
    let events: Vec<Value> = m
        .events
        .iter()
        .enumerate()
        .filter(|(i, _)| w.event_on[*i] && full.got_event[0][*i])
        .map(|(_, e)| json!([m.items[e.item as usize].name, m.regions[e.region as usize], r(&e.rule)]))
        .collect();
    let ps = &w.ps;
    let sources: Vec<Value> = ps
        .sources
        .iter()
        .enumerate()
        .filter(|(k, s)| s.on && full.sp[0].src_done[*k])
        .map(|(_, s)| {
            let regs: Vec<&String> = s.regions.iter().filter(|g| on(**g) && full.reach[0][**g as usize]).map(|g| &m.regions[*g as usize]).collect();
            json!([regs, r(&s.rule), s.mons, s.seen, s.level])
        })
        .collect();
    let evos: Vec<Value> = ps
        .evos
        .iter()
        .filter(|e| full.sp[0].caught[e.base as usize] && eval(&e.rule, &fc))
        .map(|e| json!([e.base, e.target, r(&e.rule)]))
        .collect();
    let forms: Vec<Value> = ps
        .forms
        .iter()
        .filter(|g| {
            on(g.region) && full.reach[0][g.region as usize] && g.mons.iter().any(|x| full.sp[0].caught[*x as usize])
        })
        .map(|g| {
            // a form whose change can never happen: false
            let rules: Vec<Value> = g
                .rules
                .iter()
                .map(|x| if eval(x, &fc) { r(x) } else { json!(false) })
                .collect();
            json!([m.regions[g.region as usize], g.mons, rules])
        })
        .collect();
    let mut sets_of = Map::new();
    for (mon, s) in ps.sets_of.iter().enumerate() {
        if !s.is_empty() {
            sets_of.insert(mon.to_string(), json!(s));
        }
    }
    let dex: Vec<u16> = m.facts.mons.iter().map(|x| x.0).collect();
    let (prog, useful, filler) = pool_items(m, &w);
    let id = |v: Vec<u32>| -> Vec<u32> { v.into_iter().filter_map(|i| m.items[i as usize].id).collect() };
    let vanilla: Vec<u32> = m.species.statics.iter().filter(|s| s.kind == "starter").map(|s| s.vanilla[0][0]).collect();
    let starters = if w.starters == vanilla { Vec::new() } else { w.starters.clone() };
    Ok(json!({
        "format": FORMAT,
        "generator": super::GENERATOR_VERSION,
        "data_hash": format!("{:016x}", crate::data::data_hash()),
        "menu": m.regions[m.menu as usize],
        "regions": regions,
        "rules": rules,
        "connections": conns,
        "locations": locations,
        "events": events,
        "goal": m.items[w.goal as usize].name,
        "sources": sources,
        "evolutions": evos,
        "forms": forms,
        "hall": ps.hall.filter(|g| on(*g) && full.reach[0][*g as usize]).map(|g| m.regions[g as usize].clone()),
        "sets_of": sets_of,
        "dex": dex,
        "pool": {"progression": id(prog), "useful": id(useful), "filler": id(filler)},
        "slot": {
            "version": w.version,
            "options": w.opts.0,
            "left_out": left_out,
            "starters": starters,
            "picks": ps.picks,
            "tables": Changes::of(m, &w.tables),
        },
    }))
}

/// Which locations (ids) the player reaches holding `items` (item ids,
/// repeated for counts), and whether the goal is reached.
pub fn reach_json(m: &Model, cfg: &Config, index: usize, items: &[u32]) -> Result<Value, String> {
    let p = cfg.players.get(index).ok_or("no such player")?;
    let w = build_player(m, cfg, p, index)?;
    let (inw, _) = in_world(m, &w);
    let ws = std::slice::from_ref(&w);
    let mut st = State::new(m, ws);
    for id in items {
        let i = m.item_by_id.get(id).ok_or_else(|| format!("no item {id}"))?;
        st.counts[0][*i as usize] += 1;
    }
    let none: Placement = vec![vec![None; m.locs.len()]; 1];
    sweep(m, ws, &mut st, &none);
    let mut locs: Vec<u32> = (0..m.locs.len()).filter(|&l| inw[l] && loc_ok(m, &w, &st, 0, l)).map(|l| m.locs[l].id).collect();
    locs.sort();
    Ok(json!({"locations": locs, "goal": st.counts[0][w.goal as usize] > 0}))
}
