//! Extra checks a player may ask for, with PokemonBWAP's option names:
//!
//! - `dexsanity`: catching a species is a check. A number N picks N
//!   species at random among those the player can catch; a list names the
//!   species (National Dex numbers), kept when catchable.
//! - `seensanity`: the same for seeing a species.
//! - `dexcountsanity`: `{"Maximum": M, "Steps": S, "Leniency": L}`:
//!   catching 1, 2, ... M species is a check (only counts divisible by S,
//!   and M itself); each needs L more species catchable in logic. M is cut
//!   to the number of species the player can catch.
//! - `trainersanity`: N regular trainers (no gym leader, Elite Four or
//!   story boss), picked at random among those the player can reach; a won
//!   battle is a check.
//!
//! Their locations are in the model and off; this turns on the ones the
//! options pick. Each is reachable with every item (a full sweep), so the
//! world stays finishable. Each option has its own random stream.

use super::{eval, pool_items, stream, sweep, Config, Ctx, PlayerWorld, State};
use crate::facts::LAST_DEX;
use crate::model::Model;
use serde_json::Value;

/// What a count-or-list option asks for.
enum Pick {
    Count(usize),
    List(Vec<u32>),
}

fn pick_of(o: &Value) -> Option<Pick> {
    match o {
        Value::Number(n) => n.as_u64().filter(|n| *n > 0).map(|n| Pick::Count(n as usize)),
        Value::Array(a) => {
            let v: Vec<u32> = a.iter().filter_map(|x| x.as_u64()).map(|x| x as u32).collect();
            (!v.is_empty()).then_some(Pick::List(v))
        }
        _ => None,
    }
}

fn num(o: &Value, k: &str, default: u32) -> u32 {
    o.get(k).and_then(|x| x.as_u64()).map_or(default, |x| x as u32)
}

pub(super) fn select(m: &Model, cfg: &Config, index: usize, w: &mut PlayerWorld) -> Result<(), String> {
    let get = |k: &str| w.opts.0.get(k).cloned().unwrap_or(Value::Null);
    let dex = pick_of(&get("dexsanity"));
    let seen = pick_of(&get("seensanity"));
    let count = get("dexcountsanity");
    let count_max = num(&count, "Maximum", 0).min(LAST_DEX);
    let trainers = get("trainersanity").as_u64().unwrap_or(0) as usize;
    if dex.is_none() && seen.is_none() && count_max == 0 && trainers == 0 {
        return Ok(());
    }

    // what the player can reach with every item, these checks still off
    let ws = std::slice::from_ref(&*w);
    let mut st = State::new(m, ws);
    let (prog, _, _) = pool_items(m, w);
    for i in prog {
        st.counts[0][i as usize] += 1;
    }
    sweep(m, ws, &mut st, &vec![vec![None; m.locs.len()]; 1]);
    let sp = &st.sp[0];
    let by_key = |kind: &str, key: u32| m.locs.iter().position(|l| l.kind == kind && l.key == key);
    let mut on: Vec<usize> = Vec::new();

    let mut choose = |kind: &str, pick: Option<Pick>, cands: Vec<u32>, name: &str| {
        let chosen: Vec<u32> = match pick {
            None => Vec::new(),
            Some(Pick::List(v)) => cands.iter().copied().filter(|d| v.contains(d)).collect(),
            Some(Pick::Count(n)) => {
                let mut c = cands;
                stream(cfg, name, index).shuffle(&mut c);
                c.truncate(n);
                c
            }
        };
        for k in chosen {
            if let Some(l) = by_key(kind, k) {
                on.push(l);
            }
        }
    };
    let catchable: Vec<u32> = (1..=LAST_DEX).filter(|d| sp.caught_dex[*d as usize]).collect();
    let seeable: Vec<u32> = (1..=LAST_DEX).filter(|d| sp.seen_dex[*d as usize]).collect();
    choose("dex", dex, catchable.clone(), "dexsanity");
    choose("seen", seen, seeable, "seensanity");

    let c = Ctx { m, opts: &w.opts, counts: &st.counts[0], reach: &st.reach[0], sp, ps: &w.ps };
    let mut beatable = Vec::new();
    for t in &m.species.trainers {
        let Some(l) = by_key("trainer", t.id) else { continue };
        let loc = &m.locs[l];
        if w.region_on[loc.region as usize] && st.reach[0][loc.region as usize] && eval(&t.include, &c) && eval(&loc.rule, &c) {
            beatable.push(t.id);
        }
    }
    choose("trainer", (trainers > 0).then_some(Pick::Count(trainers)), beatable, "trainersanity");

    if count_max > 0 {
        let steps = num(&count, "Steps", 1).max(1);
        let leniency = num(&count, "Leniency", 0);
        let maximum = count_max.min(catchable.len() as u32);
        for n in 1..=maximum {
            let last = n == maximum && maximum == count_max;
            if n % steps == 0 || last {
                if let Some(l) = by_key("dexcount", n) {
                    on.push(l);
                }
            }
        }
        w.ps.count_leniency = leniency;
        w.ps.count_cap = maximum;
    }
    for l in on {
        w.loc_on[l] = true;
    }
    Ok(())
}
