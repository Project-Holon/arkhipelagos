//! A player's logic rebuilt from their world data (`PlayerOut`), for the
//! mod's Tracker: which of the player's locations the items they hold
//! reach, with the generator's own rules and sweep (as `ap-reach`).
//!
//! The world data carries the player's options, the tables the randomizers
//! changed, the per-world picks and the player's locations, so the logic is
//! the one the world was made with: the tables are the original ones with
//! the changes applied, the picks are the world's, and the locations on are
//! the player's own.

use std::collections::BTreeMap;

use super::{build_player_with, loc_ok, pool_items, sweep, Config, Opts, Placement, PlayerConfig, PlayerOut, PlayerWorld, State};
use crate::facts::LAST_DEX;
use crate::model::Model;
use crate::species::Tables;
use serde_json::{Map, Value};

pub struct Logic {
    w: PlayerWorld,
    /// the model index of each of the player's locations, by location id
    pub locs: BTreeMap<u32, usize>,
    /// the items that open locations in this player's game (model indices,
    /// once each): the player's progression items
    pub progression: Vec<u32>,
}

impl Logic {
    pub fn new(m: &Model, po: &PlayerOut) -> Result<Logic, String> {
        let version = if po.version == "white" { 1 } else { 0 };
        let mut tables = Tables::vanilla(m, version);
        po.tables.apply(m, &mut tables);
        // build_player_with reads the configuration only for the picks'
        // random stream; the world's own picks are given instead
        let cfg = Config { schema: super::SCHEMA, seed: 0, options: Map::new(), players: Vec::new() };
        let p = PlayerConfig { name: po.name.clone(), version: po.version.clone(), options: Map::new() };
        let opts = Opts(po.options.clone());
        let mut w = build_player_with(m, &cfg, &p, 0, opts, tables, po.starters.clone(), Some(&po.picks))?;
        let by_id: BTreeMap<u32, usize> = m.locs.iter().enumerate().map(|(i, l)| (l.id, i)).collect();
        let locs: BTreeMap<u32, usize> =
            po.locations.iter().filter_map(|l| by_id.get(&l[0]).map(|&i| (l[0], i))).collect();
        w.loc_on = vec![false; m.locs.len()];
        for &i in locs.values() {
            w.loc_on[i] = true;
        }
        let (progression, _, _) = pool_items(m, &w);
        // the catch count checks: as sanity::select sets them
        let count = w.opts.0.get("dexcountsanity").cloned().unwrap_or(Value::Null);
        let num = |k: &str| count.get(k).and_then(Value::as_u64).unwrap_or(0) as u32;
        let count_max = num("Maximum").min(LAST_DEX);
        if count_max > 0 {
            let ws = std::slice::from_ref(&w);
            let mut st = State::new(m, ws);
            for &i in &progression {
                st.counts[0][i as usize] += 1;
            }
            sweep(m, ws, &mut st, &vec![vec![None; m.locs.len()]; 1]);
            let catchable = (1..=LAST_DEX).filter(|d| st.sp[0].caught_dex[*d as usize]).count() as u32;
            w.ps.count_leniency = num("Leniency");
            w.ps.count_cap = count_max.min(catchable);
        }
        Ok(Logic { w, locs, progression })
    }

    /// The player's locations reached holding `items` (Archipelago item
    /// ids, repeated for counts; ids that are no item of the model are
    /// left out).
    pub fn reach(&self, m: &Model, items: &[u32]) -> Vec<u32> {
        let ws = std::slice::from_ref(&self.w);
        let mut st = State::new(m, ws);
        for id in items {
            if let Some(&i) = m.item_by_id.get(id) {
                st.counts[0][i as usize] += 1;
            }
        }
        let none: Placement = vec![vec![None; m.locs.len()]; 1];
        sweep(m, ws, &mut st, &none);
        self.locs.iter().filter(|(_, &l)| loc_ok(m, &self.w, &st, 0, l)).map(|(id, _)| *id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen::{ap::reach_json, generate};

    /// The rebuilt logic reaches what the generator's own `ap-reach` does,
    /// for worlds with and without randomized tables and extra checks.
    #[test]
    fn same_as_ap_reach() {
        let m = crate::model().unwrap();
        for opts in [
            serde_json::json!({}),
            serde_json::json!({"randomize_wild_pokemon": ["randomize"], "randomize_trainer_pokemon": ["randomize"],
                               "randomize_types": ["shuffle"], "dexsanity": 30, "dexcountsanity": {"Maximum": 60, "Steps": 7, "Leniency": 2},
                               "trainersanity": 20, "season_control": "changeable"}),
        ] {
            let cfg: Config = serde_json::from_value(serde_json::json!({
                "schema": super::super::SCHEMA, "seed": 77, "options": opts,
                "players": [{"name": "A", "version": "white", "options": {}}],
            }))
            .unwrap();
            let w = generate(&m, &cfg).unwrap();
            let po = &w.players[0];
            let logic = Logic::new(&m, po).unwrap();
            // nothing, every progression item, and the first half of them
            let all: Vec<u32> = logic.progression.iter().filter_map(|&i| m.items[i as usize].id).collect();
            for items in [Vec::new(), all.clone(), all[..all.len() / 2].to_vec()] {
                let want = reach_json(&m, &cfg, 0, &items).unwrap();
                let want: Vec<u32> = want["locations"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
                let mine: Vec<u32> = po.locations.iter().map(|l| l[0]).collect();
                let want: Vec<u32> = want.into_iter().filter(|l| mine.contains(l)).collect();
                assert_eq!(logic.reach(&m, &items), want, "options {opts}, {} items", items.len());
            }
        }
    }
}
