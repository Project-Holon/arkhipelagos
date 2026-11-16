//! The Archipelago client: a game in an Archipelago room, through metaxy's
//! bridge (metaxy docs/SPEC.md, Part D) and eidos's `ap_link`, `ap_send`
//! and the event `ap`.
//!
//! - A new game started to join a room (mode `archipelago`) gets the room
//!   from eidos's `mode_code`: `ap <host:port> <slot> <password>`, the
//!   password `-` when there is none. The mod opens the link and keeps the
//!   room in the save.
//! - After the server's `RoomInfo` the mod sends `Connect` with the game of
//!   the cartridge's version. A save already played in a room only connects
//!   to the same seed (RoomInfo's `seed_name`).
//! - `Connected` carries the slot's data (the apworld's slot_data reads as
//!   the generator's `PlayerOut`). The mod stores it as a world file, so a
//!   later start applies it offline at once, and applies it as an
//!   eidos-only world is applied. Then it sends every check of the save.
//! - Items from other players come in `ReceivedItems` by index (0-based);
//!   the save keeps how many were given, so a list sent again from 0 gives
//!   nothing twice. A packet past that count asks for the whole list
//!   (`Sync`). The player's own items come from the game (items_handling 1).
//! - The names in "You found <item> for <player>!" come from
//!   `LocationScouts` (the items of other games at this player's
//!   locations) and the data package of each of those games, asked one game
//!   at a time; they are kept in a file for offline play.
//! - The goal is `StatusUpdate` 30 (CLIENT_GOAL), sent when reached and
//!   again on each connect.
//! - When the link ends the mod links again every 30 seconds, unless the
//!   room refused the slot or plays another seed.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::online::Mod;
use crate::{version_of, Game, Host, SaveState};
use arkh_gen::gen::{PlayerOut, World};

// the `ap` event's kinds (BW_AP_*)
pub const LINKED: u32 = 0;
pub const MESSAGE: u32 = 1;
pub const ERROR: u32 = 2;
pub const CLOSED: u32 = 3;

pub const GAME_BLACK: &str = "Pokemon Black (arkhipelagos)";
pub const GAME_WHITE: &str = "Pokemon White (arkhipelagos)";
/// slot_data's `format`
pub const SLOT_FORMAT: &str = "arkhipelagos-slot/1";
/// Archipelago's items_handling: items from other worlds only (this
/// player's own items are given by the game's checks)
pub const ITEMS_HANDLING: u32 = 0b001;
/// ClientStatus.CLIENT_GOAL
pub const CLIENT_GOAL: u32 = 30;
/// Frames between two link attempts (30 seconds).
pub const RETRY_FRAMES: u32 = 30 * 60;
/// The longest item name a line shows (the text box is narrow).
pub const LABEL_MAX: usize = 20;

/// The room a game plays in, kept in the save.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Room {
    /// the room's address as typed: `host:port` (or a room link)
    pub room: String,
    /// the slot's name in the room
    pub slot: String,
    /// "" when there is none
    #[serde(default)]
    pub password: String,
    /// the room's seed (RoomInfo `seed_name`), once connected
    #[serde(default)]
    pub seed: String,
    /// the Archipelago slot number, once connected
    #[serde(default)]
    pub player: u32,
}

/// Read eidos's `mode_code` line: `ap <host:port> <slot> <password>`. The
/// slot may hold spaces; the password is the last word (`-`: none).
pub fn parse_code(line: &str) -> Option<Room> {
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() < 4 || words[0] != "ap" {
        return None;
    }
    let password = words[words.len() - 1];
    Some(Room {
        room: words[1].to_string(),
        slot: words[2..words.len() - 1].join(" "),
        password: if password == "-" { String::new() } else { password.to_string() },
        ..Default::default()
    })
}

/// The Archipelago game of a cartridge.
pub fn game_name(code: &[u8; 4]) -> &'static str {
    if version_of(code) == "white" {
        GAME_WHITE
    } else {
        GAME_BLACK
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// waiting to link again (`retry` frames left)
    Waiting,
    /// the link was asked for; RoomInfo and Connected not yet
    Linking,
    /// Connect sent
    Connecting,
    Connected,
    /// the room refused this slot, or plays another seed: no more tries
    Stopped,
}

/// The names a room gives, kept in a file for offline play.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Names {
    /// player names by slot
    pub players: BTreeMap<u32, String>,
    /// the item of another game at a location of this player, by location
    pub labels: BTreeMap<u32, String>,
}

fn names_file(seed: &str, player: u32) -> String {
    format!("ap-{seed}-{player}-names.json")
}

fn world_file(seed: &str, player: u32) -> String {
    format!("ap-{seed}-{player}.json")
}

/// The client: the room and the link's state.
pub struct Client {
    pub room: Room,
    pub phase: Phase,
    pub retry: u32,
    /// slot number -> game, of this room
    pub games: BTreeMap<u32, String>,
    /// location -> (item, receiving player), the items of others found here
    pub scouted: BTreeMap<u32, (i64, u32)>,
    /// the games whose names are still to ask for, and the one asked
    pub asking: VecDeque<String>,
    pub asked: Option<String>,
    /// games whose data package ended the link (too big for the bridge):
    /// not asked again this run
    pub skip: BTreeSet<String>,
}

impl Client {
    pub fn new(room: Room) -> Client {
        Client {
            room,
            phase: Phase::Waiting,
            retry: 0,
            games: BTreeMap::new(),
            scouted: BTreeMap::new(),
            asking: VecDeque::new(),
            asked: None,
            skip: BTreeSet::new(),
        }
    }
}

/// Send one message (a JSON array of commands).
pub fn send(host: &mut dyn Host, cmds: Value) -> bool {
    host.ap_send(&cmds.to_string())
}

/// The game's lines take plain characters: names cut to `max` and
/// characters past Latin replaced.
pub fn label_text(s: &str, max: usize) -> String {
    let clean: String = s.chars().map(|c| if (' '..'\u{250}').contains(&c) { c } else { '?' }).collect();
    if clean.chars().count() <= max {
        clean
    } else {
        let mut t: String = clean.chars().take(max.saturating_sub(3)).collect();
        t.push_str("...");
        t
    }
}

/// The text of a PrintJSON (its parts' texts).
pub fn print_text(cmd: &Value) -> String {
    cmd["data"].as_array().map(|d| d.iter().filter_map(|p| p["text"].as_str()).collect()).unwrap_or_default()
}

impl Game {
    /// A check sent to the room (a connected link).
    pub(crate) fn ap_checks(&self, host: &mut dyn Host, ids: &[u32]) {
        if self.state.ap.is_some() && self.ap_live && !ids.is_empty() {
            send(host, json!([{"cmd": "LocationChecks", "locations": ids}]));
        }
    }

    /// The goal sent to the room (a connected link).
    pub(crate) fn ap_goal(&self, host: &mut dyn Host) {
        if self.state.ap.is_some() && self.ap_live && self.state.goal {
            send(host, json!([{"cmd": "StatusUpdate", "status": CLIENT_GOAL}]));
        }
    }
}

impl Mod {
    /// A new game in mode `archipelago`: the room from `mode_code`.
    pub fn ap_new_game(&mut self, host: &mut dyn Host) {
        host.member_clear_all();
        let line = host.mode_code().unwrap_or_default();
        let Some(room) = parse_code(&line) else {
            host.log("arkhipelagos: an Archipelago game without a room (mode_code); the plain game");
            return;
        };
        host.log(&format!("arkhipelagos: Archipelago room {} as {}", room.room, room.slot));
        self.ap = Some(Client::new(room));
        self.ap_link(host);
    }

    /// A loaded save that plays in a room: its world (if it was connected
    /// once) is applied already; link again.
    pub fn ap_resume(&mut self, host: &mut dyn Host, room: Room) {
        if let Some(g) = self.game.as_mut() {
            if let Some(b) = host.file_read(&names_file(&room.seed, room.player)) {
                if let Ok(n) = serde_json::from_slice::<Names>(&b) {
                    g.names = n.players;
                    g.labels = n.labels;
                }
            }
        }
        self.ap = Some(Client::new(room));
        self.ap_link(host);
    }

    fn ap_link(&mut self, host: &mut dyn Host) {
        let Some(c) = self.ap.as_mut() else { return };
        // a link of an earlier game, or a world session, may still be open
        // on the port's connection: metaxy refuses a second L on it
        host.world_leave();
        if host.ap_link(&c.room.room, &c.room.slot, &c.room.password) {
            c.phase = Phase::Linking;
        } else {
            host.log("arkhipelagos: the Archipelago link was not opened (no relay?); trying again later");
            c.phase = Phase::Waiting;
            c.retry = RETRY_FRAMES;
        }
    }

    /// Each frame: link again when it is time.
    pub fn ap_tick(&mut self, host: &mut dyn Host) {
        let Some(c) = self.ap.as_mut() else { return };
        if c.phase == Phase::Waiting {
            if c.retry > 0 {
                c.retry -= 1;
            } else {
                self.ap_link(host);
            }
        }
    }

    /// The link ended: wait and link again (unless stopped).
    pub fn ap_closed(&mut self, host: &mut dyn Host) {
        let Some(c) = self.ap.as_mut() else { return };
        if let Some(g) = self.game.as_mut() {
            g.ap_live = false;
        }
        if c.phase == Phase::Stopped {
            return;
        }
        host.log("arkhipelagos: the Archipelago link closed; linking again in 30 seconds");
        c.phase = Phase::Waiting;
        c.retry = RETRY_FRAMES;
        // a data package that ended the link (over the bridge's 2 MB) is
        // not asked for again: its items keep the "Far Item" name
        if let Some(game) = c.asked.take() {
            host.log(&format!("arkhipelagos: the link closed while {game}'s names came; not asking again"));
            c.skip.insert(game);
        }
        c.asking.clear();
    }

    /// The event `ap`.
    pub fn on_ap(&mut self, host: &mut dyn Host, kind: u32, body: &[u8]) {
        if self.ap.is_none() {
            return;
        }
        match kind {
            LINKED => host.log("arkhipelagos: linked to the Archipelago room"),
            MESSAGE => {
                let cmds: Value = match serde_json::from_slice(body) {
                    Ok(v) => v,
                    Err(e) => {
                        host.log(&format!("arkhipelagos: an Archipelago message that is not JSON: {e}"));
                        return;
                    }
                };
                // a message is an array of commands; one alone is taken too
                let list = match cmds {
                    Value::Array(a) => a,
                    v => vec![v],
                };
                for cmd in &list {
                    self.ap_command(host, cmd);
                }
            }
            ERROR => host.log(&format!("arkhipelagos: Archipelago link: {}", String::from_utf8_lossy(body))),
            CLOSED => self.ap_closed(host),
            _ => {}
        }
    }

    fn ap_command(&mut self, host: &mut dyn Host, cmd: &Value) {
        match cmd["cmd"].as_str().unwrap_or("") {
            "RoomInfo" => self.ap_room_info(host, cmd),
            "Connected" => {
                if let Err(e) = self.ap_connected(host, cmd) {
                    host.log(&format!("arkhipelagos: Connected: {e}"));
                    if let Some(c) = self.ap.as_mut() {
                        c.phase = Phase::Stopped;
                    }
                }
            }
            "ConnectionRefused" => {
                host.log(&format!("arkhipelagos: the room refused the slot: {}", cmd["errors"]));
                if let Some(c) = self.ap.as_mut() {
                    c.phase = Phase::Stopped;
                }
            }
            "ReceivedItems" => self.ap_received(host, cmd),
            "LocationInfo" => self.ap_location_info(host, cmd),
            "DataPackage" => self.ap_data_package(host, cmd),
            "PrintJSON" => {
                let t = print_text(cmd);
                host.log(&format!("arkhipelagos: room: {}", t.chars().take(200).collect::<String>()));
            }
            "InvalidPacket" => host.log(&format!("arkhipelagos: the room says: {cmd}")),
            _ => {}
        }
    }

    fn ap_room_info(&mut self, host: &mut dyn Host, cmd: &Value) {
        let code = host.game_code();
        let Some(c) = self.ap.as_mut() else { return };
        let seed = cmd["seed_name"].as_str().unwrap_or("").to_string();
        if !c.room.seed.is_empty() && c.room.seed != seed {
            host.log(&format!(
                "arkhipelagos: the room at {} plays another seed ({seed}) than this save ({}); not connecting",
                c.room.room, c.room.seed
            ));
            c.phase = Phase::Stopped;
            return;
        }
        let connect = json!([{
            "cmd": "Connect",
            "password": c.room.password,
            "game": game_name(&code),
            "name": c.room.slot,
            "uuid": format!("arkhipelagos-{}", c.room.slot),
            "version": {"class": "Version", "major": 0, "minor": 6, "build": 0},
            "items_handling": ITEMS_HANDLING,
            "tags": [],
            "slot_data": true,
        }]);
        c.room.seed = seed;
        c.phase = Phase::Connecting;
        if !send(host, connect) {
            host.log("arkhipelagos: Connect was not sent");
        }
    }

    fn ap_connected(&mut self, host: &mut dyn Host, cmd: &Value) -> Result<(), String> {
        let c = self.ap.as_mut().ok_or("no room")?;
        let me = cmd["slot"].as_u64().ok_or("no slot")? as u32;
        let team = cmd["team"].as_u64().unwrap_or(0);
        let sd = &cmd["slot_data"];
        if sd["format"].as_str() != Some(SLOT_FORMAT) {
            return Err(format!("the slot's data is not {SLOT_FORMAT} (another game, or an older apworld)"));
        }
        let mut data: PlayerOut = serde_json::from_value(sd.clone()).map_err(|e| format!("slot data: {e}"))?;
        // the slot's number in this room (slot_data's `slot` is the same)
        data.slot = me;
        let theirs = sd["data_hash"].as_str().unwrap_or("");
        let ours = format!("{:016x}", arkh_gen::data::data_hash());
        if theirs != ours {
            host.log(&format!("arkhipelagos: the room's data hash {theirs} is not the mod's {ours}"));
        }
        let players: BTreeMap<u32, String> = cmd["players"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|p| p["team"].as_u64().unwrap_or(0) == team)
                    .filter_map(|p| {
                        let name = p["alias"].as_str().or(p["name"].as_str())?;
                        Some((p["slot"].as_u64()? as u32, name.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        c.games = cmd["slot_info"]
            .as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| Some((k.parse().ok()?, v["game"].as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        c.room.player = me;
        c.phase = Phase::Connected;
        let room = c.room.clone();
        let seed = room.seed.clone();
        let world = World {
            format: "arkhipelagos-world".into(),
            schema: arkh_gen::gen::SCHEMA,
            generator: sd["generator"].as_str().unwrap_or("").to_string(),
            data_hash: theirs.to_string(),
            seed: sd["config"]["seed"].as_u64().unwrap_or(0),
            config_hash: format!("ap-{seed}"),
            players: vec![data],
        };
        let file = world_file(&seed, me);
        if !host.file_write(&file, &serde_json::to_vec(&world).map_err(|e| e.to_string())?) {
            host.log("arkhipelagos: could not store the room's data");
        }
        let mine = self
            .game
            .as_ref()
            .is_some_and(|g| g.state.ap.as_ref().is_some_and(|r| r.seed == seed && r.player == me));
        let same = mine && self.game.as_ref().is_some_and(|g| g.world == world);
        let code = host.game_code();
        if same {
            // linked again: keep the save and what is applied
        } else if mine {
            // the same slot, its data changed: keep the save's progress
            let mut state = self.game.take().ok_or("mine")?.state;
            state.world = file;
            let names = self.ap_names_of(host, &seed, me);
            let mut g = Game::new(world, me, code, state)?;
            g.labels = names.labels;
            self.game = Some(g);
            self.apply(host);
        } else {
            let state = SaveState { world: file, slot: me, ..Default::default() };
            let names = self.ap_names_of(host, &seed, me);
            let mut g = Game::new(world, me, code, state)?;
            g.labels = names.labels;
            self.game = Some(g);
            self.apply(host);
        }
        let g = self.game.as_mut().ok_or("no game")?;
        g.state.ap = Some(room);
        g.names = players;
        g.ap_live = true;
        host.log(&format!("arkhipelagos: connected to the room as slot {me}, {} locations", g.placed.len()));
        // every check of the save (the room ignores the ones it has)
        let all: Vec<u32> = g.state.checked.iter().copied().collect();
        g.ap_checks(host, &all);
        g.ap_goal(host);
        // the names for the found-for lines: what the others' items at
        // this player's locations are, then their games' names
        let foreign: Vec<u32> =
            g.placed.iter().filter(|(l, (_, to))| *to != me && !g.labels.contains_key(l)).map(|(l, _)| *l).collect();
        let c = self.ap.as_mut().ok_or("no room")?;
        c.scouted.clear();
        c.asking.clear();
        c.asked = None;
        if !foreign.is_empty() {
            send(host, json!([{"cmd": "LocationScouts", "locations": foreign, "create_as_hint": 0}]));
        }
        self.ap_store_names(host);
        Ok(())
    }

    fn ap_names_of(&self, host: &mut dyn Host, seed: &str, player: u32) -> Names {
        host.file_read(&names_file(seed, player))
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn ap_store_names(&self, host: &mut dyn Host) {
        let Some(g) = self.game.as_ref() else { return };
        let Some(r) = g.state.ap.as_ref() else { return };
        let n = Names { players: g.names.clone(), labels: g.labels.clone() };
        if let Ok(b) = serde_json::to_vec(&n) {
            host.file_write(&names_file(&r.seed, r.player), &b);
        }
    }

    fn ap_received(&mut self, host: &mut dyn Host, cmd: &Value) {
        let Some(g) = self.game.as_mut() else {
            host.log("arkhipelagos: items before the slot's data; ignored");
            return;
        };
        let index = cmd["index"].as_u64().unwrap_or(0) as u32;
        let have = g.state.received;
        if index > have {
            // items between were missed: ask for the whole list
            host.log(&format!("arkhipelagos: items from index {index}, {have} given: Sync"));
            send(host, json!([{"cmd": "Sync"}]));
            return;
        }
        let me = g.slot;
        let items = cmd["items"].as_array().cloned().unwrap_or_default();
        for (k, it) in items.iter().enumerate() {
            let at = index + k as u32;
            if at < have {
                continue;
            }
            let item = it["item"].as_i64().unwrap_or(-1);
            let from = it["player"].as_u64().unwrap_or(0) as u32;
            match u32::try_from(item) {
                Ok(item) if from == me => g.receive(host, item),
                Ok(item) => g.receive_from(host, item, from),
                Err(_) => host.log(&format!("arkhipelagos: item {item} is not one of this game's")),
            }
            g.state.received = at + 1;
        }
    }

    fn ap_location_info(&mut self, host: &mut dyn Host, cmd: &Value) {
        let Some(c) = self.ap.as_mut() else { return };
        for it in cmd["locations"].as_array().map(Vec::as_slice).unwrap_or_default() {
            let (Some(loc), Some(item), Some(to)) = (it["location"].as_u64(), it["item"].as_i64(), it["player"].as_u64()) else {
                continue;
            };
            c.scouted.insert(loc as u32, (item, to as u32));
        }
        // the games of those items, one at a time (each answer stays small)
        let games: BTreeSet<String> = c
            .scouted
            .values()
            .filter_map(|(_, to)| c.games.get(to).cloned())
            .filter(|g| !c.skip.contains(g))
            .collect();
        c.asking = games.into_iter().collect();
        self.ap_ask_next(host);
    }

    fn ap_ask_next(&mut self, host: &mut dyn Host) {
        let Some(c) = self.ap.as_mut() else { return };
        c.asked = c.asking.pop_front();
        if let Some(game) = &c.asked {
            send(host, json!([{"cmd": "GetDataPackage", "games": [game]}]));
        }
    }

    fn ap_data_package(&mut self, host: &mut dyn Host, cmd: &Value) {
        let Some(c) = self.ap.as_mut() else { return };
        let Some(games) = cmd["data"]["games"].as_object() else { return };
        let mut labels = BTreeMap::new();
        for (game, pkg) in games {
            let Some(names) = pkg["item_name_to_id"].as_object() else { continue };
            // only the ids found at this player's locations
            let want: BTreeSet<i64> = c
                .scouted
                .values()
                .filter(|(_, to)| c.games.get(to) == Some(game))
                .map(|(item, _)| *item)
                .collect();
            let by_id: BTreeMap<i64, &str> = names
                .iter()
                .filter_map(|(n, id)| id.as_i64().filter(|i| want.contains(i)).map(|i| (i, n.as_str())))
                .collect();
            for (loc, (item, to)) in &c.scouted {
                if c.games.get(to) == Some(game) {
                    if let Some(n) = by_id.get(item) {
                        labels.insert(*loc, label_text(n, LABEL_MAX));
                    }
                }
            }
        }
        if let Some(g) = self.game.as_mut() {
            g.labels.extend(labels);
        }
        self.ap_store_names(host);
        self.ap_ask_next(host);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_room_line() {
        let r = parse_code("ap 127.0.0.1:38281 Ash -").unwrap();
        assert_eq!((r.room.as_str(), r.slot.as_str(), r.password.as_str()), ("127.0.0.1:38281", "Ash", ""));
        let r = parse_code("ap archipelago.gg:38281 Red Blue hunter2").unwrap();
        assert_eq!((r.slot.as_str(), r.password.as_str()), ("Red Blue", "hunter2"));
        assert!(parse_code("ap host:1 -").is_none());
        assert!(parse_code("ABCD").is_none());
        assert!(parse_code("").is_none());
    }

    #[test]
    fn labels_fit_the_box() {
        assert_eq!(label_text("Sword", 20), "Sword");
        assert_eq!(label_text("A very long item name indeed", 20), "A very long item ...");
        assert_eq!(label_text("Pok\u{e9}mon \u{2605}", 20), "Pok\u{e9}mon ?");
    }
}
