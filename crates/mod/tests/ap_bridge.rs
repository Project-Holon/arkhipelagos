//! An Archipelago room played through a real metaxy bridge (SPEC Part D)
//! and a real Archipelago server: the host's `ap_link` and `ap_send` speak
//! metaxy's `L` and `T` frames the way eidos's port does, and the frames
//! that come back are the `ap` events.
//!
//! Set ARKH_METAXY (a metaxy with `--bridge`, and `--bridge-private true`
//! for a server on this machine) and ARKH_AP_ROOM (`host:port`) to a room
//! with an arkhipelagos slot ARKH_AP_SLOT (default "Ash", Pokemon Black,
//! goal ghetsis) that has checked nothing yet; without them the test only
//! says it was skipped. Another slot may have found items for it already.
//! The test connects, takes the items, checks two locations (one of its own
//! items and one of another game's when there is one), links again (nothing
//! given twice) and sends the goal. ARKH_NET_TRACE=1 prints the frames.

mod common;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use common::net::{token, trace, Link, V2};

use arkh_mod::ap::{self, Phase};
use arkh_mod::online::{self, Mod};
use arkh_mod::{check_table, Host};
use serde_json::Value;

#[derive(Default)]
struct Port {
    addr: String,
    link: Option<Link>,
    files: BTreeMap<String, Vec<u8>>,
    state: Option<Vec<u8>>,
    works: BTreeMap<u16, u16>,
    items: Vec<(u16, u16)>,
    log: Vec<String>,
    mode_code: String,
    sent: Vec<Value>,
}

impl Host for Port {
    fn log(&mut self, t: &str) {
        if trace() {
            eprintln!("mod: {t}");
        }
        self.log.push(t.to_string());
    }
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
    fn item_add(&mut self, item: u16, count: u16) -> bool {
        self.items.push((item, count));
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
        Some("archipelago".into())
    }
    fn mode_code(&mut self) -> Option<String> {
        Some(self.mode_code.clone())
    }
    fn ap_link(&mut self, room: &str, _slot: &str, _password: &str) -> bool {
        let mut l = Link::open(&self.addr);
        if trace() {
            eprintln!("-> L {room}");
        }
        let ok = l.send('L', format!("{V2} {}\n{room}", token()).as_bytes());
        self.link = Some(l);
        ok
    }
    fn ap_send(&mut self, json: &str) -> bool {
        if trace() {
            eprintln!("-> T {}", &json[..json.len().min(120)]);
        }
        self.sent.push(serde_json::from_str(json).unwrap());
        self.link.as_mut().is_some_and(|l| l.send('T', json.as_bytes()))
    }
}

/// Read the link until `done` holds or `wait` passes with nothing; the
/// frames become `ap` events as the port makes them.
fn pump(m: &mut Mod, h: &mut Port, wait: Duration, done: &dyn Fn(&Mod, &Port) -> bool) {
    let mut end = Instant::now() + wait;
    while Instant::now() < end && !done(m, h) {
        let Some(l) = h.link.as_mut() else { return };
        let Some((t, body)) = l.next(Duration::from_millis(150)) else {
            if l.closed {
                h.link = None;
                m.on_world(h, online::CLOSED, b"");
                return;
            }
            continue;
        };
        match t {
            'L' => m.on_ap(h, ap::LINKED, &body),
            'T' => m.on_ap(h, ap::MESSAGE, &body),
            'E' => {
                m.on_world(h, online::ERROR, &body);
                if body.starts_with(b"ap-") {
                    m.on_ap(h, ap::ERROR, &body);
                }
            }
            _ => {}
        }
        end = Instant::now() + wait;
    }
}

fn check(m: &mut Mod, h: &mut Port, loc: u32) {
    let (&(flag, key), _) = check_table().unwrap().iter().find(|(_, ls)| ls.contains(&loc)).expect("a site");
    h.works.insert(0x805F, 0xA4C7);
    h.works.insert(0x805E, key);
    h.works.insert(0x8008, 1);
    h.works.insert(0x8009, 1);
    m.game.as_mut().unwrap().on_flag(h, flag);
    h.works.insert(0x805F, 0);
}

fn printed(h: &Port, what: &str) -> bool {
    h.log.iter().any(|l| l.contains(what))
}

#[test]
fn a_room_through_the_bridge() {
    let (Ok(addr), Ok(room)) = (std::env::var("ARKH_METAXY"), std::env::var("ARKH_AP_ROOM")) else {
        eprintln!("skipped: set ARKH_METAXY and ARKH_AP_ROOM");
        return;
    };
    let slot = std::env::var("ARKH_AP_SLOT").unwrap_or_else(|_| "Ash".into());
    let mut h = Port { addr, mode_code: format!("ap {room} {slot} -"), ..Default::default() };
    let mut m = Mod::default();
    m.new_game(&mut h, 1);
    let connected = |m: &Mod, _: &Port| m.ap.as_ref().is_some_and(|c| c.phase == Phase::Connected);
    pump(&mut m, &mut h, Duration::from_secs(10), &connected);
    assert!(connected(&m, &h), "not connected: {:?}", h.log);
    // the names: scouts and one data package per other game
    let named = |m: &Mod, _: &Port| m.ap.as_ref().is_some_and(|c| c.asked.is_none() && c.asking.is_empty());
    pump(&mut m, &mut h, Duration::from_secs(3), &|m, h| named(m, h) && !m.game.as_ref().unwrap().labels.is_empty());
    let g = m.game.as_ref().unwrap();
    let me = g.slot;
    let received = g.state.received;
    assert_eq!(received as usize, h.items.len(), "every item given once");
    let foreign = g.placed.iter().find(|(_, (_, to))| *to != me).map(|(l, _)| *l);
    if let Some(l) = foreign {
        assert!(g.labels.contains_key(&l), "the other game's item is named: {:?}", h.log);
    }
    let own = *g.placed.iter().find(|(_, (item, to))| *to == me && *item != 0 && *item < 0x1000).unwrap().0;
    eprintln!("connected as slot {me}: {received} items received, {} locations, {} named", g.placed.len(), g.labels.len());

    // checks: one of this player's items, one of another game's
    let before = h.items.len();
    check(&mut m, &mut h, own);
    assert_eq!(h.items.len(), before + 1);
    if let Some(l) = foreign {
        check(&mut m, &mut h, l);
        // the server tells the room who got what
        pump(&mut m, &mut h, Duration::from_secs(3), &|_, h| printed(h, &format!("({l})")));
        assert!(printed(&h, &format!("({l})")), "no ItemSend for {l}: {:?}", h.log);
    }
    m.save(&mut h);

    // a later start of the same save: the room again, nothing twice
    let mut h2 = Port { addr: h.addr.clone(), state: h.state.clone(), files: h.files.clone(), ..Default::default() };
    if let Some(l) = h.link.as_mut() {
        l.send('B', b"");
    }
    let mut m2 = Mod::default();
    m2.save_load(&mut h2);
    pump(&mut m2, &mut h2, Duration::from_secs(10), &connected);
    assert!(connected(&m2, &h2), "not connected again: {:?}", h2.log);
    pump(&mut m2, &mut h2, Duration::from_secs(2), &|_, _| false);
    assert!(h2.items.is_empty(), "items given twice: {:?}", h2.items);
    let resent: Vec<u64> = h2
        .sent
        .iter()
        .flat_map(|m| m.as_array().cloned().unwrap_or_default())
        .filter(|c| c["cmd"] == "LocationChecks")
        .flat_map(|c| c["locations"].as_array().cloned().unwrap_or_default())
        .filter_map(|v| v.as_u64())
        .collect();
    assert!(resent.contains(&(own as u64)), "the save's checks sent again");

    // the goal: the first ending
    m2.game.as_mut().unwrap().on_cleared(&mut h2);
    pump(&mut m2, &mut h2, Duration::from_secs(3), &|_, h| printed(h, "has completed their goal"));
    assert!(printed(&h2, "has completed their goal"), "no goal: {:?}", h2.log);
    if let Some(l) = h2.link.as_mut() {
        l.send('B', b"");
    }
}
