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

use crate::{solo_config, version_of, Game, Host, SaveState};
use arkh_gen::gen::{PlayerOut, World};
use arkh_script::Family;

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
}

impl Mod {
    fn family(host: &mut dyn Host) -> Family {
        let code = host.game_code();
        Family::of_game_code(std::str::from_utf8(&code).unwrap_or("IRBO"))
    }

    fn apply(&self, host: &mut dyn Host) {
        if let Some(g) = &self.game {
            match g.apply(host) {
                Ok(n) => host.log(&format!("arkhipelagos: world {} applied ({n} members)", g.state.world)),
                Err(e) => host.log(&format!("arkhipelagos: apply failed: {e}")),
            }
        }
    }

    /// A new game: join the world of `join.txt`, else make a solo world.
    pub fn new_game(&mut self, host: &mut dyn Host, seed: u64) {
        self.game = None;
        let join = host.file_read(JOIN_FILE).and_then(|b| String::from_utf8(b).ok());
        if let Some((code, name)) = join.as_deref().and_then(|t| t.trim().split_once(' ')) {
            let mut o = Online { code: code.trim().to_string(), name: name.trim().to_string(), ..Default::default() };
            host.member_clear_all();
            // a name this game joined before goes back in by its key
            o.key = stored_key(host, &o.code, &o.name).unwrap_or_default();
            let who = if o.key.is_empty() { o.name.clone() } else { o.key.clone() };
            host.log(&format!("arkhipelagos: joining world {} as {}", o.code, o.name));
            if !host.world_join(&o.code, &who) {
                host.log("arkhipelagos: the join was not sent (no relay?)");
            }
            self.joining = Some(o);
            return;
        }
        let code = host.game_code();
        let cfg = solo_config(version_of(&code), seed, "Player", Default::default());
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
        match Game::load(host) {
            Ok(Some(g)) => {
                let online = g.state.online.clone();
                self.game = Some(g);
                self.apply(host);
                if let Some(mut o) = online {
                    // a key given after the last save is in the key file
                    if let Some(k) = stored_key(host, &o.code, &o.name) {
                        o.key = k;
                    }
                    host.world_join(&o.code, &o.key);
                    self.joining = Some(o);
                }
            }
            Ok(None) => {
                host.log("arkhipelagos: this save has no world: the plain game");
                host.member_clear_all();
                self.game = None;
            }
            Err(e) => host.log(&format!("arkhipelagos: {e}")),
        }
    }

    /// The `world` event.
    pub fn on_world(&mut self, host: &mut dyn Host, kind: u32, body: &[u8]) {
        let text = String::from_utf8_lossy(body).to_string();
        match kind {
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
                            g.receive(host, item);
                            g.state.received = index;
                        }
                    }
                }
            }
            ERROR => {
                let reason = text.split_whitespace().next().unwrap_or("");
                host.log(&format!("arkhipelagos: metaxy: {text}"));
                if reason == "bad-key" {
                    // the claim was released before it was confirmed: join
                    // again by name and keep the new key
                    if let Some(o) = self.joining.clone() {
                        host.world_join(&o.code, &o.name);
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
            let family = Mod::family(host);
            self.game = Some(Game::new(world, player, family, state)?);
            self.apply(host);
        } else {
            let state = SaveState { world: file, slot: player, online: Some(online), ..Default::default() };
            let family = Mod::family(host);
            self.game = Some(Game::new(world, player, family, state)?);
            self.apply(host);
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
