//! The mod as a whole (solo and online) and the online side: metaxy world
//! sessions (metaxy docs/SPEC.md, Part C), through eidos's world calls.
//!
//! - A new game joins the world named in the mod file `join.txt`
//!   ("<code> <name>"); without it, the game is a solo world.
//! - The join's push (`S`, the event `world` JOINED) gives this player's
//!   data. The mod stores it as a world file (so a later start works
//!   offline) and the player key, binds the save to it, and applies it.
//! - A loaded save of an online world applies the stored world at once and
//!   joins again with the key; a join refused with `bad-key` joins once more
//!   by name and keeps the new key.
//! - Checks go to metaxy (`V`) as they happen, and all of them again on each
//!   join; they are also kept in a file at once. Items for this player
//!   (`Y`) are given by index; the last index applied is in the save.
//! - The goal goes as location 0 (`V goal`).
//! - Other players: the port draws field avatars (`peer_position`); their
//!   zones become Town Map markers.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{options_from_settings, solo_config, version_of, Game, Host, SaveState};
use arkh_gen::gen::{PlayerOut, World};

// the `world` event's kinds (EIDOS_WORLD_*)
pub const GENERATED: u32 = 0;
pub const WAIT: u32 = 1;
pub const JOINED: u32 = 2;
pub const ITEMS: u32 = 3;
pub const POSITION: u32 = 4;
pub const PLAYER_IN: u32 = 5;
pub const PLAYER_OUT: u32 = 6;
pub const ERROR: u32 = 7;
pub const CLOSED: u32 = 8;
pub const OWNER_DONE: u32 = 9;

/// The owner key of a GENERATED answer ("<code> <owner key>").
fn it_owner(text: &str) -> &str {
    text.split_whitespace().nth(1).unwrap_or("")
}

/// The mod file that names the world a new game joins: "<code> <name>".
pub const JOIN_FILE: &str = "join.txt";

/// An online world, kept in the save.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Online {
    pub code: String,
    /// this player's name in the configuration
    pub name: String,
    /// the player key metaxy gave on the first join
    pub key: String,
    pub player: u32,
}

/// The player data of a join's push (SPEC section 21, "The push on join").
#[derive(Deserialize)]
struct PlayerData {
    world: String,
    player: u32,
    #[serde(default)]
    players: Vec<Value>,
    #[serde(default)]
    generator: Value,
    #[serde(default)]
    seed: u64,
    #[serde(default)]
    world_hash: String,
    data: PlayerOut,
}

fn data_file(code: &str, player: u32) -> String {
    format!("online-{}-{player}.json", code.to_ascii_uppercase())
}

/// The player key of a name in a world, written at once when metaxy gives
/// it: a game that closes before its first save keeps its claim.
fn key_file(code: &str, name: &str) -> String {
    let n: String = name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    format!("key-{}-{n}.txt", code.to_ascii_uppercase())
}

fn stored_key(host: &mut dyn Host, code: &str, name: &str) -> Option<String> {
    let t = String::from_utf8(host.file_read(&key_file(code, name))?).ok()?;
    let k = t.trim().to_string();
    (k.len() == 32).then_some(k)
}

fn checks_file(code: &str, player: u32) -> String {
    format!("checks-{}-{player}.json", code.to_ascii_uppercase())
}

/// Add checks to the world's checks file (written at once, outside the
/// game's save).
pub fn note_checks(g: &Game, host: &mut dyn Host, ids: &[u32]) {
    let Some(o) = &g.state.online else { return };
    let f = checks_file(&o.code, o.player);
    let mut all: BTreeSet<u32> =
        host.file_read(&f).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    all.extend(ids.iter().copied());
    if let Ok(b) = serde_json::to_vec(&all) {
        host.file_write(&f, &b);
    }
}

fn all_checks(g: &Game, host: &mut dyn Host) -> Vec<u32> {
    let mut all: BTreeSet<u32> = g.state.checked.clone();
    if let Some(o) = &g.state.online {
        if let Some(v) = host
            .file_read(&checks_file(&o.code, o.player))
            .and_then(|b| serde_json::from_slice::<BTreeSet<u32>>(&b).ok())
        {
            all.extend(v);
        }
    }
    all.into_iter().collect()
}

/// The mod: the running game, and a join on its way.
#[derive(Default)]
pub struct Mod {
    pub game: Option<Game>,
    /// the join sent and not answered yet (code and name, for a bad key)
    pub joining: Option<Online>,
    /// player number -> name, of this world
    pub names: BTreeMap<u32, String>,
    /// a world asked for (multiworld, create): this player's name, joined
    /// once metaxy has made it
    pub creating: Option<String>,
    /// the code of the world just made: shown to the player once joined
    pub made: Option<String>,
    /// the join on its way went by player key (a refusal then joins by name)
    pub by_key: bool,
    /// the last "Player N" tried after the player's own name (0: none yet)
    pub next_free: u32,
    /// an Archipelago room this game plays in (`ap`)
    pub ap: Option<crate::ap::Client>,
    /// eidos's field Tracker for the running game (`tracker`)
    pub tracker: Option<crate::tracker::Tracker>,
    /// the world whose Tracker could not be made (not tried again)
    pub tracker_failed: Option<String>,
}

impl Mod {
    pub(crate) fn apply(&self, host: &mut dyn Host) {
        if let Some(g) = &self.game {
            match g.apply(host) {
                Ok(n) => host.log(&format!("arkhipelagos: world {} applied ({n} members)", g.state.world)),
                Err(e) => host.log(&format!("arkhipelagos: apply failed: {e}")),
            }
        }
    }

    /// Join a world by its code and a player name (or the key that name got
    /// before on this device).
    fn join(&mut self, host: &mut dyn Host, code: &str, name: &str) {
        let mut o = Online { code: code.trim().to_ascii_uppercase(), name: name.trim().to_string(), ..Default::default() };
        host.member_clear_all();
        // a name this game joined before goes back in by its key
        o.key = stored_key(host, &o.code, &o.name).unwrap_or_default();
        let who = if o.key.is_empty() { o.name.clone() } else { o.key.clone() };
        host.log(&format!("arkhipelagos: joining world {} as {}", o.code, o.name));
        if !host.world_join(&o.code, &who) {
            host.log("arkhipelagos: the join was not sent (no relay?)");
        }
        self.by_key = !o.key.is_empty();
        self.next_free = 0;
        self.joining = Some(o);
    }

    /// A new game.  On an eidos with modes, the mode the player chose on
    /// the Multiplayer screen decides: "solo" makes a solo world from the
    /// screen's options, "multiworld" joins a world by code and name or
    /// asks metaxy to make one, and the plain New Game (no mode) is the
    /// plain game.  Without modes (an older eidos), and for test runs,
    /// `join.txt` joins its world and anything else is a solo world.
    pub fn new_game(&mut self, host: &mut dyn Host, seed: u64) {
        self.game = None;
        self.creating = None;
        self.tracker_reset();
        if self.ap.take().is_some() {
            // the room of the game before this one: its link ends
            host.world_leave();
        }
        let mode = host.mode();
        let join = host.file_read(JOIN_FILE).and_then(|b| String::from_utf8(b).ok());
        let join = join.as_deref().and_then(|t| t.trim().split_once(' ')).map(|(c, n)| (c.to_string(), n.to_string()));
        let code = host.game_code();
        let options = match mode.as_deref() {
            Some("multiworld") => {
                // the name the player chose on the Multiplayer screen: never asked again here
                let name = host.player_name().unwrap_or_default();
                let world = host.mode_code().unwrap_or_default();
                if !world.trim().is_empty() {
                    // join: the code typed in eidos's Join box (eidos asked
                    // the relay first that it names an open world with room)
                    if name.trim().is_empty() {
                        host.log("arkhipelagos: a multiworld join needs a multiplayer name; the plain game");
                        host.member_clear_all();
                        return;
                    }
                    self.join(host, world.trim(), &name);
                    return;
                }
                // create: this player and the others, every one on this
                // cartridge's version; joiners take the names Player 2 ...
                let n = host.setting("arkh.players").unwrap_or(2).clamp(1, 16) as usize;
                let me = if name.trim().is_empty() { "Player 1".to_string() } else { name.trim().to_string() };
                let mut cfg = solo_config(version_of(&code), seed, &me, options_from_settings(host));
                for k in 2..=n {
                    let mut p = cfg.players[0].clone();
                    p.name = format!("Player {k}");
                    cfg.players.push(p);
                }
                // open names: a joiner's own multiplayer name takes the next
                // free place (metaxy SPEC), so nobody plays as "Player 2"
                let json = serde_json::to_value(&cfg).map(|mut v| {
                    if let Some(o) = v.as_object_mut() {
                        o.insert("open_names".into(), serde_json::Value::Bool(true));
                    }
                    v.to_string()
                });
                match json {
                    Ok(json) if host.world_create(&json) => {
                        host.log(&format!("arkhipelagos: making a world for {n} players"));
                        host.member_clear_all();
                        self.creating = Some(me);
                    }
                    _ => host.log("arkhipelagos: the world was not asked for (no relay?)"),
                }
                return;
            }
            Some("solo") => options_from_settings(host),
            Some("archipelago") => {
                // a game in an Archipelago room (eidos's Join box gave the
                // room, the slot and the password)
                self.ap_new_game(host);
                return;
            }
            Some(_) => {
                // the plain New Game (or another mod's mode): no world,
                // unless a test run names one in join.txt
                if let Some((c, n)) = join {
                    self.join(host, &c, &n);
                    return;
                }
                host.log("arkhipelagos: a plain new game (the Multiplayer screen's cards start a randomizer)");
                host.member_clear_all();
                return;
            }
            None => {
                if let Some((c, n)) = join {
                    self.join(host, &c, &n);
                    return;
                }
                Default::default()
            }
        };
        let cfg = solo_config(version_of(&code), seed, "Player", options);
        match Game::create_solo(host, &cfg) {
            Ok(g) => {
                self.game = Some(g);
                self.apply(host);
            }
            Err(e) => host.log(&format!("arkhipelagos: {e}")),
        }
    }

    /// A loaded save: its world applied at once (offline too); an online
    /// world is joined again with the player key.
    pub fn save_load(&mut self, host: &mut dyn Host) {
        self.joining = None;
        self.tracker_reset();
        if self.ap.take().is_some() {
            host.world_leave();
        }
        match Game::load(host) {
            Ok(Some(g)) => {
                let online = g.state.online.clone();
                let room = g.state.ap.clone();
                self.game = Some(g);
                self.apply(host);
                if let Some(room) = room {
                    // an Archipelago game: the room again (same seed only)
                    self.ap_resume(host, room);
                }
                if let Some(mut o) = online {
                    // a key given after the last save is in the key file
                    if let Some(k) = stored_key(host, &o.code, &o.name) {
                        o.key = k;
                    }
                    host.world_join(&o.code, &o.key);
                    self.by_key = true;
                    self.next_free = 0;
                    self.joining = Some(o);
                }
            }
            Ok(None) => {
                host.member_clear_all();
                self.game = None;
                // a game saved in a room before its first connect
                let room = host
                    .state_load()
                    .and_then(|b| serde_json::from_slice::<SaveState>(&b).ok())
                    .and_then(|s| s.ap);
                match room {
                    Some(room) => self.ap_resume(host, room),
                    None => host.log("arkhipelagos: this save has no world: the plain game"),
                }
            }
            Err(e) => host.log(&format!("arkhipelagos: {e}")),
        }
    }

    /// The `world` event.
    /// `save.begin`: the game's state, or (a room not connected yet) the room.
    pub fn save(&self, host: &mut dyn Host) {
        if let Some(g) = &self.game {
            g.save(host);
        } else if let Some(c) = &self.ap {
            let s = SaveState { ap: Some(c.room.clone()), ..Default::default() };
            if let Ok(b) = serde_json::to_vec(&s) {
                host.state_store(&b);
            }
        }
    }

    /// Each frame: the Archipelago link's retries, the game's messages, the
    /// Tracker.
    pub fn tick(&mut self, host: &mut dyn Host) {
        self.ap_tick(host);
        if let Some(g) = self.game.as_mut() {
            g.tick(host);
        }
        self.tracker_tick(host);
    }

    /// eidos clears the Tracker at a new game and a load: send it again.
    fn tracker_reset(&mut self) {
        if let Some(t) = self.tracker.as_mut() {
            t.sent = None;
            t.dirty = true;
            t.wait = 0;
        }
    }

    fn tracker_tick(&mut self, host: &mut dyn Host) {
        let Some(g) = self.game.as_ref() else { return };
        let world = g.state.world.clone();
        if self.tracker.as_ref().map(|t| t.world.as_str()) != Some(world.as_str())
            && self.tracker_failed.as_deref() != Some(world.as_str())
        {
            self.tracker = match crate::tracker::Tracker::new(g.player(), &world) {
                Ok(t) => Some(t),
                Err(e) => {
                    host.log(&format!("arkhipelagos: no tracker: {e}"));
                    self.tracker_failed = Some(world);
                    None
                }
            };
        }
        if let Some(t) = self.tracker.as_mut() {
            if t.world == g.state.world {
                t.tick(host, g);
            }
        }
    }

    /// The event `world`.
    pub fn on_world(&mut self, host: &mut dyn Host, kind: u32, body: &[u8]) {
        let text = String::from_utf8_lossy(body).to_string();
        if self.ap.is_some() {
            // the Archipelago link is a world session of the port: its end
            // comes as the world's CLOSED, and errors without the `ap-`
            // prefix (busy, version) only as the world's ERROR
            match kind {
                CLOSED => return self.ap_closed(host),
                ERROR => {
                    if !text.starts_with("ap-") {
                        host.log(&format!("arkhipelagos: Archipelago link: {text}"));
                    }
                    return;
                }
                _ => {}
            }
        }
        match kind {
            GENERATED => {
                // "<code> <owner key>": the world asked for is made
                let mut it = text.split_whitespace();
                let Some(code) = it.next().map(|c| c.to_ascii_uppercase()) else { return };
                if let Some(owner) = it.next() {
                    if !host.file_write(&format!("owner-{code}.txt"), owner.as_bytes()) {
                        host.log("arkhipelagos: could not store the world's owner key");
                    }
                }
                host.log(&format!("arkhipelagos: world {code} made"));
                if self.creating.is_some() {
                    // nobody joins a world before its owner starts it
                    self.made = Some(code.clone());
                    if !host.world_start(&code, it_owner(&text)) {
                        host.log("arkhipelagos: the world's start was not sent");
                    }
                }
            }
            OWNER_DONE => {
                // "<code> started": this player joins the world it made
                let mut it = text.split_whitespace();
                let (code, state) = (it.next().unwrap_or("").to_ascii_uppercase(), it.next().unwrap_or(""));
                if state == "started" && self.made.as_deref() == Some(code.as_str()) {
                    if let Some(me) = self.creating.take() {
                        self.join(host, &code, &me);
                    }
                }
            }
            JOINED => {
                if let Err(e) = self.joined(host, &text) {
                    host.log(&format!("arkhipelagos: join: {e}"));
                }
            }
            ITEMS => {
                let Some(g) = self.game.as_mut() else { return };
                let n: Vec<u32> = text.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                for t in n.chunks(3) {
                    if let [index, item, _from] = *t {
                        if index > g.state.received {
                            g.receive_from(host, item, _from);
                            g.state.received = index;
                        }
                    }
                }
            }
            ERROR => {
                let reason = text.split_whitespace().next().unwrap_or("");
                host.log(&format!("arkhipelagos: metaxy: {text}"));
                if reason == "bad-key" || reason == "name-taken" {
                    if let Some(mut o) = self.joining.clone() {
                        if self.by_key {
                            // the claim was released before it was confirmed:
                            // join again by name and keep the new key
                            self.by_key = false;
                            host.world_join(&o.code, &o.name);
                        } else if self.next_free < 16 {
                            // the multiplayer name is not one of the world's
                            // players, or is taken: the next "Player N" of a
                            // world made in the game
                            self.next_free = if self.next_free == 0 { 2 } else { self.next_free + 1 };
                            o.name = format!("Player {}", self.next_free);
                            host.log(&format!("arkhipelagos: joining world {} as {}", o.code, o.name));
                            host.world_join(&o.code, &o.name);
                            self.joining = Some(o);
                        } else {
                            host.log("arkhipelagos: no free player in this world");
                            self.joining = None;
                        }
                    }
                }
            }
            POSITION => {
                let f: Vec<i64> = text.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                if let [player, zone, ..] = *f.as_slice() {
                    let name = self.names.get(&(player as u32)).cloned().unwrap_or_default();
                    host.townmap_marker_set(player as u32, zone as u32, &name);
                }
            }
            PLAYER_OUT => {
                if let Some(p) = text.split_whitespace().next().and_then(|x| x.parse().ok()) {
                    host.townmap_marker_clear(p);
                }
            }
            CLOSED => host.log("arkhipelagos: the world link closed; checks are kept and sent on the next join"),
            _ => {}
        }
    }

    fn joined(&mut self, host: &mut dyn Host, text: &str) -> Result<(), String> {
        let (head, json) = text.split_once('\n').ok_or("no player data")?;
        let mut h = head.split_whitespace();
        let player: u32 = h.next().and_then(|x| x.parse().ok()).ok_or("no player")?;
        let key = h.next().ok_or("no key")?.to_string();
        let pd: PlayerData = serde_json::from_str(json).map_err(|e| format!("player data: {e}"))?;
        if pd.player != player || pd.data.slot != player {
            return Err("the push names another player".into());
        }
        self.names = pd
            .players
            .iter()
            .filter_map(|p| Some((p["player"].as_u64()? as u32, p["name"].as_str()?.to_string())))
            .collect();
        let theirs = pd.generator["data_hash"].as_str().unwrap_or("");
        let ours = format!("{:016x}", arkh_gen::data::data_hash());
        if theirs != ours {
            // metaxy's generator is another build: its world may place items
            // where this mod has no site
            host.log(&format!("arkhipelagos: the world's data hash {theirs} is not the mod's {ours}"));
        }
        let name = self.names.get(&player).cloned().unwrap_or_else(|| pd.data.name.clone());
        let online = Online { code: pd.world.to_ascii_uppercase(), name, key, player };
        let world = World {
            format: "arkhipelagos-world".into(),
            schema: arkh_gen::gen::SCHEMA,
            generator: pd.generator["version"].as_str().unwrap_or("").to_string(),
            data_hash: pd.generator["data_hash"].as_str().unwrap_or("").to_string(),
            seed: pd.seed,
            config_hash: pd.world_hash.clone(),
            players: vec![pd.data],
        };
        if !host.file_write(&key_file(&online.code, &online.name), online.key.as_bytes()) {
            host.log("arkhipelagos: could not store the player key");
        }
        let file = data_file(&online.code, player);
        let mine = self
            .game
            .as_ref()
            .is_some_and(|g| g.state.online.as_ref().is_some_and(|o| o.code == online.code && o.player == player));
        let same = mine && self.game.as_ref().is_some_and(|g| g.world == world);
        if !host.file_write(&file, &serde_json::to_vec(&world).map_err(|e| e.to_string())?) {
            host.log("arkhipelagos: could not store the world's data");
        }
        if same {
            // a join again: keep the save, take the (maybe new) key
            if let Some(g) = self.game.as_mut() {
                g.state.online = Some(online);
            }
        } else if mine {
            // the same player's world, sent anew: keep the save's progress
            let mut state = self.game.take().expect("mine").state;
            state.online = Some(online);
            state.world = file;
            let code = host.game_code();
            self.game = Some(Game::new(world, player, code, state)?);
            self.apply(host);
        } else {
            let state = SaveState { world: file, slot: player, online: Some(online), ..Default::default() };
            let code = host.game_code();
            self.game = Some(Game::new(world, player, code, state)?);
            self.apply(host);
        }
        if let Some(g) = self.game.as_mut() {
            g.names = self.names.clone();
            // a world this player just made: the code to give the others
            if self.made.as_deref() == Some(g.state.online.as_ref().map(|o| o.code.as_str()).unwrap_or("")) {
                let code = self.made.take().unwrap_or_default();
                g.messages.push_back(crate::Message::Line { id: "world.made", item: 0, name: code, label: None });
            }
        }
        self.joining = None;
        let g = self.game.as_ref().expect("set above");
        let all = all_checks(g, host);
        if !all.is_empty() {
            host.world_checks(&all);
        }
        if g.state.goal {
            host.world_checks(&[0]);
        }
        host.peer_position(true);
        Ok(())
    }
}
