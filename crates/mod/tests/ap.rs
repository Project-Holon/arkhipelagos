//! The Archipelago client with recorded messages: a room of this world
//! (default options, Pokemon Black, slot "Ash") and APQuest (slot "Quest"),
//! made with Archipelago 0.6.8 and the apworld, and the messages its server
//! sent (tests/ap/*.json). The host records what the mod sends.

use std::collections::BTreeMap;

use arkh_mod::ap::{self, Phase, CLIENT_GOAL, RETRY_FRAMES};
use arkh_mod::online::Mod;
use arkh_mod::{check_table, Host, SaveState, LINE_BANK, LINE_SCRIPT};
use serde_json::{json, Value};

const ROOM_INFO: &str = include_str!("ap/room_info.json");
const CONNECTED: &str = include_str!("ap/connected.json");
const LOCATION_INFO: &str = include_str!("ap/location_info.json");
const DATA_PACKAGE: &str = include_str!("ap/data_package.json");
const PRINT_JSON: &str = include_str!("ap/print_json.json");

/// Opelucid Gym's guide: APQuest's Shield in the recorded room
const FOREIGN_LOC: u32 = 67536;
/// Route 1's woman: this player's Wide Lens (265)
const OWN_LOC: u32 = 67648;

#[derive(Default)]
struct Fake {
    code: [u8; 4],
    mode: String,
    mode_code: String,
    files: BTreeMap<String, Vec<u8>>,
    state: Option<Vec<u8>>,
    works: BTreeMap<u16, u16>,
    items: Vec<(u16, u16)>,
    texts: Vec<(u32, u32, u32, u32, Vec<u16>)>,
    scripts: Vec<u16>,
    links: Vec<(String, String, String)>,
    leaves: u32,
    link_ok: bool,
    sent: Vec<Value>,
    log: Vec<String>,
}

impl Fake {
    fn new() -> Fake {
        Fake {
            code: *b"IRBO",
            mode: "archipelago".into(),
            mode_code: "ap 127.0.0.1:38281 Ash -".into(),
            link_ok: true,
            ..Default::default()
        }
    }
    /// the commands sent, by name
    fn sent_cmds(&self, name: &str) -> Vec<Value> {
        self.sent.iter().flat_map(|m| m.as_array().cloned().unwrap_or_default()).filter(|c| c["cmd"] == name).collect()
    }
    fn text_of(units: &[u16]) -> String {
        char::decode_utf16(units.iter().copied().filter(|&u| u < 0xF000)).map(|c| c.unwrap_or('?')).collect()
    }
}

impl Host for Fake {
    fn log(&mut self, t: &str) {
        self.log.push(t.to_string());
    }
    fn member_get(&mut self, a: &str, i: u32) -> Option<Vec<u8>> {
        // no cartridge (apply is tested elsewhere); 862's text bank header
        (a == "a/0/0/3" && i == LINE_BANK).then(|| vec![1, 0, 15, 0, 0, 0, 0, 0])
    }
    fn member_set(&mut self, _a: &str, _i: u32, _d: &[u8]) -> bool {
        true
    }
    fn member_clear_all(&mut self) {}
    fn type_chart_set(&mut self, _c: &[u8]) -> bool {
        true
    }
    fn work_get(&mut self, id: u16) -> Option<u16> {
        Some(*self.works.get(&id).unwrap_or(&0))
    }
    fn work_set(&mut self, id: u16, v: u16) -> bool {
        self.works.insert(id, v);
        true
    }
    fn item_add(&mut self, item: u16, count: u16) -> bool {
        self.items.push((item, count));
        true
    }
    fn badge_set(&mut self, _bit: u32) -> bool {
        true
    }
    fn script_run(&mut self, id: u16) -> Option<u32> {
        self.scripts.push(id);
        Some(1)
    }
    fn text_set(&mut self, a: u32, f: u32, e: u32, b: u32, u: &[u16]) -> bool {
        self.texts.push((a, f, e, b, u.to_vec()));
        true
    }
    fn state_load(&mut self) -> Option<Vec<u8>> {
        self.state.clone()
    }
    fn state_store(&mut self, d: &[u8]) -> bool {
        self.state = Some(d.to_vec());
        true
    }
    fn file_read(&mut self, n: &str) -> Option<Vec<u8>> {
        self.files.get(n).cloned()
    }
    fn file_write(&mut self, n: &str, d: &[u8]) -> bool {
        self.files.insert(n.to_string(), d.to_vec());
        true
    }
    fn game_code(&mut self) -> [u8; 4] {
        self.code
    }
    fn mode(&mut self) -> Option<String> {
        Some(self.mode.clone())
    }
    fn mode_code(&mut self) -> Option<String> {
        Some(self.mode_code.clone())
    }
    fn ap_link(&mut self, room: &str, slot: &str, password: &str) -> bool {
        self.links.push((room.into(), slot.into(), password.into()));
        self.link_ok
    }
    fn world_leave(&mut self) {
        self.leaves += 1;
    }
    fn ap_send(&mut self, json: &str) -> bool {
        self.sent.push(serde_json::from_str(json).expect("the mod sends JSON"));
        true
    }
}

fn msg(m: &mut Mod, h: &mut Fake, text: &str) {
    m.on_ap(h, ap::MESSAGE, text.as_bytes());
}

/// A check through `flag.set`: the site of `loc` with the scripts' marker.
fn check(m: &mut Mod, h: &mut Fake, loc: u32) {
    let (&(flag, key), _) = check_table().unwrap().iter().find(|(_, ls)| ls.contains(&loc)).expect("a site of the location");
    h.works.insert(0x805F, 0xA4C7);
    h.works.insert(0x805E, key);
    h.works.insert(0x8008, 1);
    h.works.insert(0x8009, 1);
    assert!(m.game.as_mut().unwrap().on_flag(h, flag));
    h.works.insert(0x805F, 0);
}

/// Show every waiting message (each line's script ends at once).
fn show_all(m: &mut Mod, h: &mut Fake) -> Vec<String> {
    let before = h.texts.len();
    m.tick(h);
    for _ in 0..16 {
        let g = m.game.as_mut().unwrap();
        if !g.showing {
            break;
        }
        // the line script ended: the next message starts
        g.script_done(h);
    }
    h.texts[before..].iter().map(|t| Fake::text_of(&t.4)).collect()
}

/// A new game in the room, connected with the recorded messages.
fn connected() -> (Mod, Fake) {
    let mut h = Fake::new();
    let mut m = Mod::default();
    m.new_game(&mut h, 1);
    m.on_ap(&mut h, ap::LINKED, b"ok");
    msg(&mut m, &mut h, ROOM_INFO);
    msg(&mut m, &mut h, CONNECTED);
    (m, h)
}

#[test]
fn a_new_game_links_and_connects() {
    let mut h = Fake::new();
    let mut m = Mod::default();
    m.new_game(&mut h, 1);
    assert_eq!(h.links, vec![("127.0.0.1:38281".to_string(), "Ash".to_string(), String::new())]);
    assert!(m.game.is_none(), "no world before Connected");
    m.on_ap(&mut h, ap::LINKED, b"ok");
    assert!(h.sent.is_empty(), "Connect waits for RoomInfo");
    msg(&mut m, &mut h, ROOM_INFO);
    let c = h.sent_cmds("Connect");
    assert_eq!(c.len(), 1);
    let c = &c[0];
    assert_eq!(c["game"], "Pokemon Black (arkhipelagos)");
    assert_eq!(c["name"], "Ash");
    assert_eq!(c["password"], "");
    assert_eq!(c["items_handling"], 1);
    assert_eq!(c["slot_data"], true);
    assert_eq!(c["tags"], json!([]));
    assert_eq!(c["version"]["class"], "Version");

    msg(&mut m, &mut h, CONNECTED);
    let g = m.game.as_ref().expect("the slot's world");
    assert_eq!(g.slot, 1);
    assert_eq!(g.placed.len(), 617);
    assert_eq!(g.placed[&FOREIGN_LOC], (0, 2));
    assert_eq!(g.placed[&OWN_LOC], (265, 1));
    assert_eq!(g.names.get(&2).map(String::as_str), Some("Quest"));
    assert!(g.ap_live);
    let room = g.state.ap.clone().unwrap();
    assert_eq!((room.seed.as_str(), room.player), ("35931773795037525048", 1));
    assert!(h.files.contains_key("ap-35931773795037525048-1.json"), "the world kept for offline play");
    // the two items Quest had found: given, with the sent-you lines
    assert_eq!(h.items, vec![(251, 1), (459, 1)]);
    assert_eq!(g.state.received, 2);
    // the others' items at this player's locations are scouted
    let s = h.sent_cmds("LocationScouts");
    assert_eq!(s.len(), 1);
    assert_eq!(s[0]["locations"].as_array().unwrap().len(), 6);
    assert_eq!(s[0]["create_as_hint"], 0);
    // no checks yet: none sent
    assert!(h.sent_cmds("LocationChecks").is_empty());

    msg(&mut m, &mut h, LOCATION_INFO);
    let d = h.sent_cmds("GetDataPackage");
    assert_eq!(d, vec![json!({"cmd": "GetDataPackage", "games": ["APQuest"]})]);
    msg(&mut m, &mut h, DATA_PACKAGE);
    let g = m.game.as_ref().unwrap();
    assert_eq!(g.labels.get(&FOREIGN_LOC).map(String::as_str), Some("Shield"));
    assert_eq!(g.labels.len(), 6);
    assert!(h.files.contains_key("ap-35931773795037525048-1-names.json"));

    // a PrintJSON is logged as text
    msg(&mut m, &mut h, PRINT_JSON);
    assert!(h.log.iter().any(|l| l.contains("room: 1 sent 3 to 2 (67536)")), "{:?}", h.log);

    // the lines: both received items, from Quest
    let shown = show_all(&mut m, &mut h);
    assert_eq!(shown.len(), 2, "{shown:?}");
    assert!(shown.iter().all(|s| s.starts_with("Quest sent you")), "{shown:?}");
    assert!(h.scripts.iter().all(|&s| s == LINE_SCRIPT));
}

#[test]
fn checks_go_to_the_room_with_names() {
    let (mut m, mut h) = connected();
    msg(&mut m, &mut h, LOCATION_INFO);
    msg(&mut m, &mut h, DATA_PACKAGE);
    show_all(&mut m, &mut h);
    h.items.clear();
    h.texts.clear();

    // a location holding APQuest's Shield: a found-for line, nothing given
    check(&mut m, &mut h, FOREIGN_LOC);
    assert!(h.items.is_empty());
    assert_eq!(h.sent_cmds("LocationChecks"), vec![json!({"cmd": "LocationChecks", "locations": [FOREIGN_LOC]})]);
    let shown = show_all(&mut m, &mut h);
    // (the line break is a control unit, left out here)
    assert_eq!(shown, vec!["You found Shieldfor Quest!".to_string()], "{shown:?}");

    // this player's own item: given at once, and sent as a check
    check(&mut m, &mut h, OWN_LOC);
    assert_eq!(h.items, vec![(265, 1)]);
    assert_eq!(h.sent_cmds("LocationChecks").len(), 2);
    // the same site again gives the game's own item, sends nothing
    check(&mut m, &mut h, OWN_LOC);
    assert_eq!(h.sent_cmds("LocationChecks").len(), 2);

    // the goal: the first ending (goal ghetsis)
    m.game.as_mut().unwrap().on_cleared(&mut h);
    assert_eq!(h.sent_cmds("StatusUpdate"), vec![json!({"cmd": "StatusUpdate", "status": CLIENT_GOAL})]);
}

#[test]
fn items_by_index() {
    let (mut m, mut h) = connected();
    assert_eq!(h.items.len(), 2);
    let items: Value = serde_json::from_str::<Value>(CONNECTED).unwrap()[1]["items"].clone();
    // the whole list again (a Sync, a reconnect): nothing twice
    let again = json!([{"cmd": "ReceivedItems", "index": 0, "items": items}]).to_string();
    msg(&mut m, &mut h, &again);
    assert_eq!(h.items.len(), 2);
    // the next item
    let next = json!([{"cmd": "ReceivedItems", "index": 2, "items": [{"item": 280, "location": 5, "player": 2, "flags": 0}]}]);
    msg(&mut m, &mut h, &next.to_string());
    assert_eq!(h.items.last(), Some(&(280, 1)));
    assert_eq!(m.game.as_ref().unwrap().state.received, 3);
    // one past what was given: ask for the whole list
    let gap = json!([{"cmd": "ReceivedItems", "index": 5, "items": [{"item": 17, "location": 6, "player": 2, "flags": 0}]}]);
    msg(&mut m, &mut h, &gap.to_string());
    assert_eq!(h.items.len(), 3);
    assert_eq!(h.sent_cmds("Sync").len(), 1);
}

#[test]
fn a_saved_game_links_again_to_its_seed() {
    let (mut m, mut h) = connected();
    msg(&mut m, &mut h, LOCATION_INFO);
    msg(&mut m, &mut h, DATA_PACKAGE);
    check(&mut m, &mut h, OWN_LOC);
    m.game.as_mut().unwrap().on_cleared(&mut h);
    m.save(&mut h);
    let saved: SaveState = serde_json::from_slice(h.state.as_ref().unwrap()).unwrap();
    assert_eq!(saved.ap.as_ref().unwrap().seed, "35931773795037525048");
    assert_eq!(saved.received, 2);

    // a later start: the save's world at once (offline), then the room
    let mut h2 = Fake { state: h.state.clone(), files: h.files.clone(), ..Fake::new() };
    let mut m2 = Mod::default();
    m2.save_load(&mut h2);
    let g = m2.game.as_ref().expect("the stored world");
    assert!(g.state.checked.contains(&OWN_LOC));
    assert_eq!(g.labels.get(&FOREIGN_LOC).map(String::as_str), Some("Shield"), "names kept offline");
    assert_eq!(h2.links.len(), 1);
    msg(&mut m2, &mut h2, ROOM_INFO);
    assert_eq!(h2.sent_cmds("Connect").len(), 1);
    msg(&mut m2, &mut h2, CONNECTED);
    // nothing given twice; the save's checks and goal sent again
    assert!(h2.items.is_empty(), "{:?}", h2.items);
    assert_eq!(h2.sent_cmds("LocationChecks"), vec![json!({"cmd": "LocationChecks", "locations": [OWN_LOC]})]);
    assert_eq!(h2.sent_cmds("StatusUpdate").len(), 1);
    // the labels known already: no scouts
    assert!(h2.sent_cmds("LocationScouts").is_empty());

    // the same address now serving another seed: no Connect
    let mut h3 = Fake { state: h.state.clone(), files: h.files.clone(), ..Fake::new() };
    let mut m3 = Mod::default();
    m3.save_load(&mut h3);
    let other = ROOM_INFO.replace("35931773795037525048", "11111111111111111111");
    msg(&mut m3, &mut h3, &other);
    assert!(h3.sent_cmds("Connect").is_empty());
    assert_eq!(m3.ap.as_ref().unwrap().phase, Phase::Stopped);
}

#[test]
fn a_save_before_the_first_connect_keeps_the_room() {
    let mut h = Fake { link_ok: false, ..Fake::new() };
    let mut m = Mod::default();
    m.new_game(&mut h, 1);
    m.save(&mut h);
    let mut h2 = Fake { state: h.state.clone(), ..Fake::new() };
    let mut m2 = Mod::default();
    m2.save_load(&mut h2);
    assert!(m2.game.is_none());
    assert_eq!(h2.links, vec![("127.0.0.1:38281".to_string(), "Ash".to_string(), String::new())]);
}

#[test]
fn the_link_comes_back() {
    let (mut m, mut h) = connected();
    // the port's world session ended (eidos gives it as the world's CLOSED)
    m.on_world(&mut h, arkh_mod::online::CLOSED, b"");
    assert!(!m.game.as_ref().unwrap().ap_live);
    // a check while closed is kept and not sent
    check(&mut m, &mut h, OWN_LOC);
    assert!(h.sent_cmds("LocationChecks").is_empty());
    for _ in 0..RETRY_FRAMES {
        m.tick(&mut h);
    }
    assert_eq!(h.links.len(), 1);
    m.tick(&mut h);
    assert_eq!(h.links.len(), 2, "linked again after 30 seconds");
    msg(&mut m, &mut h, ROOM_INFO);
    msg(&mut m, &mut h, CONNECTED);
    assert_eq!(h.sent_cmds("LocationChecks"), vec![json!({"cmd": "LocationChecks", "locations": [OWN_LOC]})]);

    // a refused slot is not tried again
    let refused = json!([{"cmd": "ConnectionRefused", "errors": ["InvalidSlot"]}]).to_string();
    msg(&mut m, &mut h, &refused);
    m.on_world(&mut h, arkh_mod::online::CLOSED, b"");
    for _ in 0..RETRY_FRAMES * 2 {
        m.tick(&mut h);
    }
    assert_eq!(h.links.len(), 2);
}

#[test]
fn white_connects_as_white_and_a_line_without_a_room_is_the_plain_game() {
    let mut h = Fake { code: *b"IRAO", ..Fake::new() };
    let mut m = Mod::default();
    m.new_game(&mut h, 1);
    msg(&mut m, &mut h, ROOM_INFO);
    assert_eq!(h.sent_cmds("Connect")[0]["game"], "Pokemon White (arkhipelagos)");

    let mut h = Fake { mode_code: String::new(), ..Fake::new() };
    let mut m = Mod::default();
    m.new_game(&mut h, 1);
    assert!(h.links.is_empty());
    assert!(m.ap.is_none());
}

#[test]
fn an_unnamed_item_of_another_game_is_a_far_item() {
    let (mut m, mut h) = connected();
    show_all(&mut m, &mut h);
    // checked before the names came: the cartridge's "Far Item"
    check(&mut m, &mut h, FOREIGN_LOC);
    assert_eq!(show_all(&mut m, &mut h), vec!["You found Far Itemfor Quest!".to_string()]);
}

#[test]
fn a_second_game_leaves_the_first_link() {
    let (mut m, mut h) = connected();
    m.save(&mut h);
    let links = h.links.len();
    // Continue on the same run: the earlier link ends before the new one
    let leaves = h.leaves;
    m.save_load(&mut h);
    assert!(h.leaves > leaves);
    assert_eq!(h.links.len(), links + 1);
    m.save_load(&mut h);
    assert_eq!(h.links.len(), links + 2);
    // a plain new game after it: the room's link ends, none opens
    let leaves = h.leaves;
    h.mode = String::new();
    m.new_game(&mut h, 1);
    assert!(h.leaves > leaves);
    assert!(m.ap.is_none());
    assert_eq!(h.links.len(), links + 2);
}

#[test]
fn a_data_package_that_ends_the_link_is_not_asked_again() {
    let (mut m, mut h) = connected();
    msg(&mut m, &mut h, LOCATION_INFO);
    assert_eq!(h.sent_cmds("GetDataPackage").len(), 1);
    // the answer was too big for the bridge: the link closed
    m.on_world(&mut h, arkh_mod::online::CLOSED, b"");
    for _ in 0..=RETRY_FRAMES {
        m.tick(&mut h);
    }
    msg(&mut m, &mut h, ROOM_INFO);
    msg(&mut m, &mut h, CONNECTED);
    msg(&mut m, &mut h, LOCATION_INFO);
    assert_eq!(h.sent_cmds("GetDataPackage").len(), 1, "APQuest not asked again");
}
