//! The solo flow with a stub host: make a configuration, generate, store,
//! load back, apply (when a cartridge's data is at hand), and every check
//! of the world turned into its item.
//!
//! Set ARKH_CART_DATA to a cartridge's extracted data folder (with its game
//! code in ARKH_CART_CODE, default IRBO) to also apply the world to it.

use std::collections::BTreeMap;

use arkh_formats::narc::Narc;
use arkh_mod::{solo_config, Game, Host, SaveState, BADGE_ITEM, MSG_COUNT_WORK, MSG_ITEM_WORK, MSG_SCRIPT};

#[derive(Default)]
struct Stub {
    code: [u8; 4],
    data: Option<String>,
    narcs: BTreeMap<String, Narc>,
    set: BTreeMap<(String, u32), Vec<u8>>,
    works: BTreeMap<u16, u16>,
    files: BTreeMap<String, Vec<u8>>,
    state: Option<Vec<u8>>,
    items: Vec<(u16, u16)>,
    badges: Vec<u32>,
    scripts: Vec<u16>,
    chart: Option<Vec<u8>>,
    bag_full: bool,
}

impl Host for Stub {
    fn log(&mut self, _text: &str) {}
    fn member_get(&mut self, archive: &str, index: u32) -> Option<Vec<u8>> {
        if let Some(d) = self.set.get(&(archive.to_string(), index)) {
            return Some(d.clone());
        }
        let dir = self.data.clone()?;
        if !self.narcs.contains_key(archive) {
            let raw = std::fs::read(format!("{dir}/{archive}")).ok()?;
            self.narcs.insert(archive.to_string(), Narc::parse(&raw).ok()?);
        }
        self.narcs[archive].files.get(index as usize).cloned()
    }
    fn member_set(&mut self, archive: &str, index: u32, data: &[u8]) -> bool {
        self.set.insert((archive.to_string(), index), data.to_vec());
        true
    }
    fn member_clear_all(&mut self) {
        self.set.clear();
    }
    fn type_chart_set(&mut self, chart: &[u8]) -> bool {
        self.chart = Some(chart.to_vec());
        chart.len() == 289
    }
    fn work_get(&mut self, id: u16) -> Option<u16> {
        Some(*self.works.get(&id).unwrap_or(&0))
    }
    fn work_set(&mut self, id: u16, value: u16) -> bool {
        self.works.insert(id, value);
        true
    }
    fn item_add(&mut self, item: u16, count: u16) -> bool {
        if self.bag_full {
            return false;
        }
        self.items.push((item, count));
        true
    }
    fn badge_set(&mut self, bit: u32) -> bool {
        self.badges.push(bit);
        true
    }
    fn script_run(&mut self, id: u16) -> Option<u32> {
        self.scripts.push(id);
        Some(self.scripts.len() as u32)
    }
    fn state_load(&mut self) -> Option<Vec<u8>> {
        self.state.clone()
    }
    fn state_store(&mut self, data: &[u8]) -> bool {
        self.state = Some(data.to_vec());
        true
    }
    fn file_read(&mut self, name: &str) -> Option<Vec<u8>> {
        self.files.get(name).cloned()
    }
    fn file_write(&mut self, name: &str, data: &[u8]) -> bool {
        self.files.insert(name.to_string(), data.to_vec());
        true
    }
    fn game_code(&mut self) -> [u8; 4] {
        self.code
    }
}

/// What the edited script does at a check: works, then SetEventFlag.
fn fire(g: &mut Game, h: &mut Stub, flag: u16, key: u16, orig: (u16, u16)) -> bool {
    h.works.insert(0x805E, key);
    h.works.insert(0x805F, 0xA4C7);
    h.works.insert(0x8008, orig.0);
    h.works.insert(0x8009, orig.1);
    let r = g.on_flag(h, flag);
    h.works.insert(0x805F, 0);
    r
}

fn stub() -> Stub {
    let code = std::env::var("ARKH_CART_CODE").unwrap_or_else(|_| "IRBO".into());
    Stub { code: code.as_bytes().try_into().unwrap(), data: std::env::var("ARKH_CART_DATA").ok(), ..Default::default() }
}

#[test]
fn solo_world_checks_give_their_items() {
    let mut h = stub();
    let cfg = solo_config("black", 4242, "Player", Default::default());
    let mut g = Game::create_solo(&mut h, &cfg).expect("world");
    assert_eq!(g.state.slot, 1);
    assert!(h.files.contains_key(&g.state.world), "the world is stored in the mod's files");

    // a flag set without the marker is the game's own
    assert!(!g.on_flag(&mut h, 1230));
    assert!(h.items.is_empty());

    // every check of the table, in order
    let table = g.checks.clone();
    let mut want_items = 0;
    let mut want_badges = 0;
    let mut checks = 0;
    for ((flag, key), locs) in &table {
        let before = (h.items.len(), h.badges.len());
        assert!(fire(&mut g, &mut h, *flag, *key, (1, 1)));
        let mine: Vec<u32> = locs.iter().copied().filter(|l| g.placed.contains_key(l)).collect();
        if mine.is_empty() {
            // not in the world: the original item (or the badge)
            assert!(h.items.len() + h.badges.len() > before.0 + before.1, "flag {flag} key {key}: original given");
            continue;
        }
        for l in &mine {
            let (item, to) = g.placed[l];
            assert_eq!(to, 1, "solo: every item is the player's");
            checks += 1;
            if (BADGE_ITEM..BADGE_ITEM + 8).contains(&item) {
                want_badges += 1;
            } else if item < 0x1000 {
                want_items += 1;
            }
        }
        // a second time gives nothing
        let after = (h.items.len(), h.badges.len());
        assert!(fire(&mut g, &mut h, *flag, *key, (1, 1)));
        assert_eq!((h.items.len(), h.badges.len()), after, "flag {flag}: checked twice");
    }
    let in_world = g.player().locations.len();
    assert_eq!(checks, in_world, "every location of the world has a check");
    assert_eq!(g.state.checked.len(), in_world);
    let world_items = h.items.iter().filter(|(_, c)| *c == 1).count();
    assert!(world_items >= want_items, "{world_items} items given, {want_items} wanted");
    assert!(h.badges.len() >= want_badges);

    // messages: one script at a time, the works set for it
    g.tick(&mut h);
    assert_eq!(h.scripts, vec![MSG_SCRIPT]);
    let first = h.items[0];
    assert_eq!((h.works[&MSG_ITEM_WORK], h.works[&MSG_COUNT_WORK]), first);
    g.tick(&mut h);
    assert_eq!(h.scripts.len(), 1, "no second message while one shows");
    g.script_done(&mut h);
    assert_eq!(h.scripts.len(), 2);

    // the save keeps the checks; loading gives the same game back
    let loaded = Game::load(&mut h).expect("load").expect("a world");
    assert_eq!(loaded.state, g.state);
    assert_eq!(loaded.world, g.world);
}

#[test]
fn full_bag_waits() {
    let mut h = stub();
    let cfg = solo_config("white", 7, "Player", Default::default());
    let mut g = Game::create_solo(&mut h, &cfg).expect("world");
    let (&(flag, key), _) = g
        .checks
        .iter()
        .find(|(_, ls)| ls.iter().any(|l| g.placed.get(l).is_some_and(|&(i, _)| i < 0x1000)))
        .expect("a check with a game item");
    h.bag_full = true;
    fire(&mut g, &mut h, flag, key, (1, 1));
    assert!(h.items.is_empty());
    assert!(!g.state.pending.is_empty());
    h.bag_full = false;
    g.tick(&mut h);
    assert!(!h.items.is_empty() && g.state.pending.is_empty());
}

#[test]
fn no_world_no_game() {
    let mut h = stub();
    assert!(Game::load(&mut h).unwrap().is_none());
    h.state = Some(serde_json::to_vec(&SaveState::default()).unwrap());
    assert!(Game::load(&mut h).unwrap().is_none());
}

#[test]
fn apply_to_a_cartridge() {
    let mut h = stub();
    if h.data.is_none() {
        eprintln!("ARKH_CART_DATA not set: apply not tested");
        return;
    }
    let mut cfg = solo_config(arkh_mod::version_of(&h.code), 99, "Player", Default::default());
    cfg.options.insert("randomize_type_chart".into(), serde_json::json!(["shuffle"]));
    cfg.options.insert("randomize_wild_pokemon".into(), serde_json::json!(["randomize"]));
    let g = Game::create_solo(&mut h, &cfg).expect("world");
    let n = g.apply(&mut h).expect("apply");
    let scripts = h.set.keys().filter(|(a, _)| a == "a/0/5/7").count();
    assert_eq!(scripts, 78, "the check edits");
    assert!(n > scripts, "and the tables");
    assert!(h.chart.is_some(), "the type chart");
    // applying again builds on the originals, not on the first apply
    let n2 = g.apply(&mut h).expect("apply again");
    assert_eq!(n, n2);
}
