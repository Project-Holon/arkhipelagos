//! The arkhipelagos eidos mod, solo flow: make a configuration, generate
//! the world, apply it to the player's own files, turn checks into items.
//!
//! The logic is here, over the `Host` trait, so it runs and is tested
//! natively; `wasm.rs` binds it to eidos's API (docs/v6-calls.md).
//!
//! A check (the contract with the edited scripts, crates/apply/src/scripts.rs):
//! the script sets a location's flag with work 0x805F = 0xA4C7, the key in
//! 0x805E, and the original item and count in 0x8008 and 0x8009. The
//! `flag.set` handler maps (flag, key) to location ids through
//! data/world/item_sites.json and gives what the world placed there.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use arkh_apply::scripts::{BADGE_FLAG, KEY_WORK, MARK, MARK_WORK};
use arkh_gen::gen::{Config, PlayerConfig, PlayerOut, World};
use arkh_script::Family;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[cfg(target_arch = "wasm32")]
mod wasm;

/// The saved works the message script reads (unused by the game).
pub const MSG_ITEM_WORK: u16 = 0x40FE;
pub const MSG_COUNT_WORK: u16 = 0x40FF;
/// The message script (862 entry 15): "You obtained X!" from those works.
pub const MSG_SCRIPT: u16 = 2815;
/// Item ids of the world that are not game items.
pub const BADGE_ITEM: u32 = 0x1000;
pub const SEASON_ITEM: u32 = 0x1100;

/// What the mod needs from eidos. `wasm.rs` implements it with the API's
/// imports; tests implement it in memory.
pub trait Host {
    fn log(&mut self, text: &str);
    /// the member as the game would read it now
    fn member_get(&mut self, archive: &str, index: u32) -> Option<Vec<u8>>;
    fn member_set(&mut self, archive: &str, index: u32, data: &[u8]) -> bool;
    /// drop every member this mod set
    fn member_clear_all(&mut self);
    fn type_chart_set(&mut self, chart: &[u8]) -> bool;
    /// a work variable (script-local ones only while a script runs)
    fn work_get(&mut self, id: u16) -> Option<u16>;
    fn work_set(&mut self, id: u16, value: u16) -> bool;
    fn item_add(&mut self, item: u16, count: u16) -> bool;
    fn badge_set(&mut self, bit: u32) -> bool;
    /// queue a field script; it runs when the field is idle
    fn script_run(&mut self, id: u16) -> Option<u32>;
    /// the mod's state, kept with the save
    fn state_load(&mut self) -> Option<Vec<u8>>;
    fn state_store(&mut self, data: &[u8]) -> bool;
    /// the mod's own files, shared by all saves
    fn file_read(&mut self, name: &str) -> Option<Vec<u8>>;
    fn file_write(&mut self, name: &str, data: &[u8]) -> bool;
    /// the cartridge's game code ("IRBO" and so on)
    fn game_code(&mut self) -> [u8; 4];
}

/// What the mod keeps with the save.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct SaveState {
    /// the file holding this save's world (`world-<hash>.json`)
    pub world: String,
    pub slot: u32,
    /// locations checked, by id
    pub checked: BTreeSet<u32>,
    /// items received from others, by index (online)
    pub received: u32,
    /// items the bag could not take yet: (item, count)
    pub pending: Vec<(u16, u16)>,
}

/// One running game: the world, this player's part, the check table.
pub struct Game {
    pub world: World,
    pub slot: u32,
    pub family: Family,
    pub state: SaveState,
    /// (game flag, key) -> location ids
    pub checks: BTreeMap<(u16, u16), Vec<u32>>,
    /// location id -> (item id, receiving slot), this player's locations
    pub placed: BTreeMap<u32, (u32, u32)>,
    /// messages waiting for the field: (item, count)
    pub messages: VecDeque<(u16, u16)>,
    /// a message script is running
    pub showing: bool,
}

/// The check table from item_sites.json: (game flag, key) -> location ids.
pub fn check_table() -> Result<BTreeMap<(u16, u16), Vec<u32>>, String> {
    let sites: Value = serde_json::from_str(arkh_gen::data::ITEM_SITES).map_err(|e| e.to_string())?;
    let mut out: BTreeMap<(u16, u16), Vec<u32>> = BTreeMap::new();
    for (lid, s) in sites["sites"].as_object().ok_or("item_sites.json: sites")? {
        let id: u32 = lid.parse().map_err(|_| format!("location {lid}"))?;
        let flag = s.get("game_flag").or(s.get("flag")).and_then(Value::as_u64).ok_or("site flag")? as u16;
        let key = s["key"].as_u64().ok_or("site key")? as u16;
        out.entry((flag, key)).or_default().push(id);
    }
    Ok(out)
}

/// The version a game code names.
pub fn version_of(code: &[u8; 4]) -> &'static str {
    if code[2] == b'A' {
        "white"
    } else {
        "black"
    }
}

/// A solo configuration: one player on this cartridge's version.
pub fn solo_config(version: &str, seed: u64, name: &str, options: Map<String, Value>) -> Config {
    Config {
        schema: arkh_gen::gen::SCHEMA,
        seed,
        options,
        players: vec![PlayerConfig { name: name.to_string(), version: version.to_string(), options: Map::new() }],
    }
}

/// The name of a world's file: its configuration's hash.
pub fn world_file(w: &World) -> String {
    format!("world-{}.json", w.config_hash)
}

impl Game {
    pub fn new(world: World, slot: u32, family: Family, state: SaveState) -> Result<Game, String> {
        let p = world.players.iter().find(|p| p.slot == slot).ok_or("no such slot in the world")?;
        let placed = p.locations.iter().map(|l| (l[0], (l[1], l[2]))).collect();
        Ok(Game { slot, family, state, checks: check_table()?, placed, messages: VecDeque::new(), showing: false, world })
    }

    pub fn player(&self) -> &PlayerOut {
        self.world.players.iter().find(|p| p.slot == self.slot).expect("slot checked in new")
    }

    /// Make a solo world and store it in the mod's files.
    pub fn create_solo(host: &mut dyn Host, cfg: &Config) -> Result<Game, String> {
        let code = host.game_code();
        let m = arkh_gen::model()?;
        let world = arkh_gen::generate(&m, cfg)?;
        let file = world_file(&world);
        let text = serde_json::to_vec(&world).map_err(|e| e.to_string())?;
        if !host.file_write(&file, &text) {
            return Err(format!("could not store {file}"));
        }
        let state = SaveState { world: file, slot: 1, ..Default::default() };
        let g = Game::new(world, 1, Family::of_game_code(std::str::from_utf8(&code).unwrap_or("IRBO")), state)?;
        g.save(host);
        Ok(g)
    }

    /// The game of a loaded save, or None when the save has no world.
    pub fn load(host: &mut dyn Host) -> Result<Option<Game>, String> {
        let Some(raw) = host.state_load() else { return Ok(None) };
        let state: SaveState = serde_json::from_slice(&raw).map_err(|e| format!("saved state: {e}"))?;
        if state.world.is_empty() {
            return Ok(None);
        }
        let text = host.file_read(&state.world).ok_or(format!("the world file {} is missing", state.world))?;
        let world: World = serde_json::from_slice(&text).map_err(|e| format!("{}: {e}", state.world))?;
        let code = host.game_code();
        let family = Family::of_game_code(std::str::from_utf8(&code).unwrap_or("IRBO"));
        let slot = state.slot;
        Game::new(world, slot, family, state).map(Some)
    }

    pub fn save(&self, host: &mut dyn Host) {
        if let Ok(b) = serde_json::to_vec(&self.state) {
            if !host.state_store(&b) {
                host.log("arkhipelagos: could not store the state");
            }
        }
    }

    /// Put the world into the player's files: the randomizers' tables, the
    /// check edits and the message script, the type chart. Returns the
    /// number of members set.
    pub fn apply(&self, host: &mut dyn Host) -> Result<usize, String> {
        let p = self.player().clone();
        // build on the cartridge's own members, not on an earlier apply
        host.member_clear_all();
        let cell = std::cell::RefCell::new(host);
        let mut sets: Vec<(&str, u32, Vec<u8>)> = Vec::new();
        for arc in arkh_apply::ARCHIVES {
            let get = |i: u32| cell.borrow_mut().member_get(arc, i);
            for (i, d) in arkh_apply::members(&p.tables, arc, &get)? {
                sets.push((arc, i, d));
            }
        }
        let scripts = arkh_apply::scripts::SCRIPTS;
        let get = |i: u32| cell.borrow_mut().member_get(scripts, i);
        let ws = arkh_apply::scripts::WorldSpecies { version: &p.version, changes: &p.tables, starters: &p.starters, picks: &p.picks };
        for (i, d) in arkh_apply::scripts::members_for(self.family, Some(&ws), &get)? {
            sets.push((scripts, i, d));
        }
        let host = cell.into_inner();
        for (arc, i, d) in &sets {
            if !host.member_set(arc, *i, d) {
                return Err(format!("{arc} {i}: the member was not taken"));
            }
        }
        if let Some(chart) = arkh_apply::type_chart(&p.tables) {
            if !host.type_chart_set(&chart) {
                return Err("the type chart was not taken".into());
            }
        }
        Ok(sets.len())
    }

    /// The `flag.set` handler. Returns whether it was a check.
    pub fn on_flag(&mut self, host: &mut dyn Host, flag: u16) -> bool {
        if host.work_get(MARK_WORK as u16) != Some(MARK as u16) {
            return false;
        }
        let key = host.work_get(KEY_WORK as u16).unwrap_or(0);
        let orig_item = host.work_get(0x8008).unwrap_or(0);
        let orig_count = host.work_get(0x8009).unwrap_or(0);
        let locs = self.checks.get(&(flag, key)).cloned().unwrap_or_default();
        let mine: Vec<u32> = locs.iter().copied().filter(|l| self.placed.contains_key(l)).collect();
        if mine.is_empty() || mine.iter().all(|l| self.state.checked.contains(l)) {
            // not a location of this world, or one already checked (a gift
            // the game gives again, as the Driftveil lady does each day):
            // the game's own item
            self.give_original(host, flag, orig_item, orig_count);
            return true;
        }
        for l in mine {
            if !self.state.checked.insert(l) {
                continue;
            }
            let (item, to) = self.placed[&l];
            if to == self.slot {
                self.receive(host, item);
            }
            // items for other players go out with the online flow
        }
        self.save(host);
        true
    }

    fn give_original(&mut self, host: &mut dyn Host, flag: u16, item: u16, count: u16) {
        if item == 0 && (BADGE_FLAG..BADGE_FLAG + 8).contains(&(flag as u32)) {
            host.badge_set(flag as u32 - BADGE_FLAG);
        } else if item != 0 {
            self.give(host, item, count.max(1));
        }
    }

    /// An item of the world reaches this player.
    pub fn receive(&mut self, host: &mut dyn Host, item: u32) {
        if (BADGE_ITEM..BADGE_ITEM + 8).contains(&item) {
            host.badge_set(item - BADGE_ITEM);
        } else if (SEASON_ITEM..SEASON_ITEM + 4).contains(&item) {
            // season items open the season choice (season.change, later)
            host.log(&format!("arkhipelagos: season item {}", item - SEASON_ITEM));
        } else if item < 0x1000 {
            self.give(host, item as u16, 1);
        }
    }

    fn give(&mut self, host: &mut dyn Host, item: u16, count: u16) {
        if host.item_add(item, count) {
            self.messages.push_back((item, count));
        } else {
            self.state.pending.push((item, count));
        }
    }

    /// Called when the field may be idle (each frame, or after a message
    /// ended): retries the bag, starts the next message.
    pub fn tick(&mut self, host: &mut dyn Host) {
        if !self.state.pending.is_empty() {
            let pend = std::mem::take(&mut self.state.pending);
            for (item, count) in pend {
                self.give(host, item, count);
            }
        }
        if self.showing {
            return;
        }
        if let Some((item, count)) = self.messages.pop_front() {
            host.work_set(MSG_ITEM_WORK, item);
            host.work_set(MSG_COUNT_WORK, count);
            if host.script_run(MSG_SCRIPT).is_some() {
                self.showing = true;
            }
        }
    }

    /// The message script ended (`script.done`).
    pub fn script_done(&mut self, host: &mut dyn Host) {
        self.showing = false;
        self.tick(host);
    }
}
