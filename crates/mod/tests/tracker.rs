//! eidos's field Tracker: the text the mod sends for a solo world, its
//! areas and zones, found, open and locked checks, the items, the count,
//! and when it is sent.

use std::collections::BTreeMap;
use std::time::Instant;

use arkh_mod::online::Mod;
use arkh_mod::tracker::{Tracker, SEND_FRAMES};
use arkh_mod::{solo_config, Game, Host};

#[derive(Default)]
struct Fake {
    files: BTreeMap<String, Vec<u8>>,
    state: Option<Vec<u8>>,
    works: BTreeMap<u16, u16>,
    sent: Vec<String>,
}

impl Host for Fake {
    fn log(&mut self, _t: &str) {}
    fn member_get(&mut self, _a: &str, _i: u32) -> Option<Vec<u8>> {
        None
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
    fn item_add(&mut self, _item: u16, _count: u16) -> bool {
        true
    }
    fn badge_set(&mut self, _bit: u32) -> bool {
        true
    }
    fn script_run(&mut self, _id: u16) -> Option<u32> {
        Some(1)
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
        *b"IRBO"
    }
    fn mode(&mut self) -> Option<String> {
        Some("solo".into())
    }
    fn tracker_set(&mut self, text: &str) -> bool {
        self.sent.push(text.to_string());
        true
    }
}

fn lines<'a>(text: &'a str, kind: &str) -> Vec<Vec<&'a str>> {
    text.lines().map(|l| l.split('\t').collect::<Vec<_>>()).filter(|w| w[0] == kind).collect()
}

fn solo(host: &mut Fake) -> Game {
    let cfg = solo_config("black", 5, "Player", Default::default());
    Game::create_solo(host, &cfg).unwrap()
}

#[test]
fn the_text_of_a_solo_world() {
    let mut h = Fake::default();
    let mut g = solo(&mut h);
    let t0 = Instant::now();
    let t = Tracker::new(g.player(), &g.state.world).unwrap();
    let built = t0.elapsed();
    let t1 = Instant::now();
    let text = t.text(&g);
    let made = t1.elapsed();
    eprintln!("tracker: made in {built:?}, text in {made:?}, {} bytes", text.len());

    let areas = lines(&text, "area");
    let checks = lines(&text, "check");
    assert_eq!(checks.len(), g.placed.len(), "every location of the player once");
    assert!(areas.len() > 50 && areas.len() <= 160, "{} areas", areas.len());
    assert!(areas.iter().all(|a| a.len() == 3 && a[1].len() < 40));
    // Route 1 is zone 317; a gym has its own zone
    assert!(areas.iter().any(|a| a[1] == "Route 1" && a[2] == "317"), "{areas:?}");
    assert!(areas.iter().any(|a| a[1] == "Striaton Gym" && a[2] == "7"));
    // nothing held: some open, some locked, none found
    let state = |c: &Vec<&str>| c[1].to_string();
    assert!(checks.iter().any(|c| state(c) == "open"));
    assert!(checks.iter().any(|c| state(c) == "locked"));
    assert!(!checks.iter().any(|c| state(c) == "found"));
    assert_eq!(lines(&text, "count"), vec![vec!["count", "0", &g.placed.len().to_string()]]);
    // the items, by group, none held
    let items = lines(&text, "item");
    for group in ["Badges", "HMs", "Key items"] {
        assert!(items.iter().any(|i| i[1] == group), "{group}");
    }
    assert!(items.iter().any(|i| i[1] == "Badges" && i[3] == "Trio Badge" && i[2] == "0"));
    let open = |text: &str| lines(text, "check").iter().filter(|c| c[1] == "open").count();
    let open0 = open(&text);

    // a check found, and every progression item held: more open
    let first = *g.placed.keys().next().unwrap();
    g.state.checked.insert(first);
    g.state.got = t.progression_ids();
    let t2 = Instant::now();
    let text2 = t.text(&g);
    eprintln!("tracker: text with every item in {:?}", t2.elapsed());
    assert_eq!(lines(&text2, "count")[0][1], "1");
    assert!(lines(&text2, "check").iter().any(|c| c[1] == "found"));
    assert!(open(&text2) > open0, "{} open with every item, {open0} with none", open(&text2));
    assert!(lines(&text2, "item").iter().any(|i| i[3] == "Trio Badge" && i[2] == "1"));
    assert!(!lines(&text2, "check").iter().any(|c| c[1] == "locked"), "every item opens every location");
}

#[test]
fn sent_at_most_once_a_second_and_only_when_changed() {
    let mut h = Fake::default();
    let mut m = Mod::default();
    m.new_game(&mut h, 5);
    assert!(m.game.is_some());
    m.tick(&mut h);
    assert_eq!(h.sent.len(), 1, "sent on the first frame of the game");
    for _ in 0..SEND_FRAMES * 2 {
        m.tick(&mut h);
    }
    assert_eq!(h.sent.len(), 1, "nothing changed");
    // a check: sent again once the second is over
    let g = m.game.as_mut().unwrap();
    let first = *g.placed.keys().next().unwrap();
    g.state.checked.insert(first);
    m.tick(&mut h);
    assert_eq!(h.sent.len(), 2);
    let g = m.game.as_mut().unwrap();
    let second = *g.placed.keys().nth(1).unwrap();
    g.state.checked.insert(second);
    m.tick(&mut h);
    assert_eq!(h.sent.len(), 2, "within the second");
    for _ in 0..SEND_FRAMES {
        m.tick(&mut h);
    }
    assert_eq!(h.sent.len(), 3);
    // a load clears eidos's Tracker: sent again though the same
    m.save(&mut h);
    m.save_load(&mut h);
    m.tick(&mut h);
    assert_eq!(h.sent.len(), 4);
    assert_eq!(h.sent[3], h.sent[2]);
}
