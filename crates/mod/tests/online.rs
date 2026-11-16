//! A two-player online world played end to end against a real metaxy
//! (SPEC Part C): make and start the world, both players join, checks
//! cross, items arrive with their indexes, a player rejoins by key (and
//! again: nothing twice), positions become Town Map markers, both goals.
//!
//! The hosts' world calls speak metaxy's frames the way eidos's port does
//! (`I` join, `V` checks, the push `S` as the event JOINED, `Y` as ITEMS,
//! `U` as POSITION). Run with ARKH_METAXY=127.0.0.1:20609; without it the
//! test only says it was skipped.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use arkh_mod::online::{self, Mod};
use arkh_mod::Host;

const V2: &str = "eidos-net/2";

struct Link {
    s: TcpStream,
    buf: Vec<u8>,
    pieces: BTreeMap<String, Vec<u8>>,
}

impl Link {
    fn open(addr: &str) -> Link {
        let s = TcpStream::connect(addr).expect("metaxy");
        s.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        Link { s, buf: Vec::new(), pieces: BTreeMap::new() }
    }
    /// false when metaxy closed the connection
    fn send(&mut self, t: char, body: &[u8]) -> bool {
        let mut f = format!("{t} {}\n", body.len()).into_bytes();
        f.extend_from_slice(body);
        self.s.write_all(&f).is_ok()
    }
    /// The next whole frame, big payloads put back together; None after
    /// `wait` with nothing.
    fn next(&mut self, wait: Duration) -> Option<(char, Vec<u8>)> {
        let end = Instant::now() + wait;
        loop {
            if let Some(nl) = self.buf.iter().position(|&b| b == b'\n') {
                let head = String::from_utf8_lossy(&self.buf[..nl]).to_string();
                let (t, len) = head.split_once(' ').unwrap();
                let len: usize = len.parse().unwrap();
                if self.buf.len() >= nl + 1 + len {
                    let body = self.buf[nl + 1..nl + 1 + len].to_vec();
                    self.buf.drain(..nl + 1 + len);
                    let t = t.chars().next().unwrap();
                    if t == 'F' {
                        let p = body.iter().position(|&b| b == b'\n').unwrap();
                        let h = String::from_utf8_lossy(&body[..p]).to_string();
                        let id = h.split(' ').next().unwrap().to_string();
                        self.pieces.entry(id).or_default().extend_from_slice(&body[p + 1..]);
                        continue;
                    }
                    // `@<id>` in place of the body (after the first line for S)
                    let text = String::from_utf8_lossy(&body).to_string();
                    let (first, rest) = text.split_once('\n').map(|(a, b)| (a.to_string(), Some(b.to_string()))).unwrap_or((text.clone(), None));
                    let at = rest.as_deref().unwrap_or(&first).to_string();
                    if let Some(id) = at.strip_prefix('@') {
                        if let Some(whole) = self.pieces.remove(id) {
                            let body = match rest {
                                Some(_) => [format!("{first}\n").into_bytes(), whole].concat(),
                                None => whole,
                            };
                            return Some((t, body));
                        }
                    }
                    return Some((t, body));
                }
            }
            if Instant::now() > end {
                return None;
            }
            let mut b = [0u8; 65536];
            match self.s.read(&mut b) {
                Ok(0) => return None,
                Ok(n) => self.buf.extend_from_slice(&b[..n]),
                Err(_) => {}
            }
        }
    }
}

fn token() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!("{:032x}", t ^ 0x5a5a_1234_9876_abcd_u128)
}

/// A game: the stub state, its files, and the port's side of the link.
#[derive(Default)]
struct Net {
    addr: String,
    link: Option<Link>,
    sent: BTreeSet<u32>,
    files: BTreeMap<String, Vec<u8>>,
    state: Option<Vec<u8>>,
    works: BTreeMap<u16, u16>,
    items: Vec<(u16, u16)>,
    badges: Vec<u32>,
    markers: BTreeMap<u32, u32>,
    log: Vec<String>,
    code: [u8; 4],
}

impl Host for Net {
    fn log(&mut self, t: &str) {
        self.log.push(t.to_string());
    }
    fn member_get(&mut self, _a: &str, _i: u32) -> Option<Vec<u8>> {
        None // no cartridge here: apply is tested elsewhere
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
    fn badge_set(&mut self, bit: u32) -> bool {
        self.badges.push(bit);
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
        self.code
    }
    fn world_join(&mut self, code: &str, who: &str) -> bool {
        let mut l = Link::open(&self.addr);
        l.send('I', format!("{V2} {} {code} {who}", token()).as_bytes());
        self.link = Some(l);
        self.sent.clear();
        true
    }
    fn world_checks(&mut self, ids: &[u32]) -> bool {
        let Some(l) = self.link.as_mut() else { return false };
        let ok = if ids == [0] {
            l.send('V', b"goal")
        } else {
            let body: Vec<String> = ids.iter().map(|x| x.to_string()).collect();
            l.send('V', body.join(" ").as_bytes())
        };
        if !ok {
            // as the port: the link is gone; the mod's checks file keeps them
            self.log.push("link lost".into());
            self.link = None;
            return false;
        }
        if ids != [0] {
            self.sent.extend(ids);
        }
        true
    }
    fn world_leave(&mut self) {
        if let Some(mut l) = self.link.take() {
            l.send('B', b"");
        }
    }
    fn townmap_marker_set(&mut self, id: u32, zone: u32, _n: &str) {
        self.markers.insert(id, zone);
    }
    fn townmap_marker_clear(&mut self, id: u32) {
        self.markers.remove(&id);
    }
}

/// Read the link for a while and hand the frames to the mod as the port's
/// world events; after a join's push, send the session's checks (`V`, empty
/// when there are none), as the port does.
fn pump(m: &mut Mod, h: &mut Net, wait: Duration) {
    pump_as(m, h, wait, true)
}

/// `confirm` false: the port's V after the push is not sent (a game that
/// closes before it confirms its claim).
fn pump_as(m: &mut Mod, h: &mut Net, wait: Duration, confirm: bool) {
    let end = Instant::now() + wait;
    while Instant::now() < end {
        let Some(l) = h.link.as_mut() else { return };
        let Some((t, body)) = l.next(Duration::from_millis(150)) else { continue };
        match t {
            'S' => {
                m.on_world(h, online::JOINED, &body);
                if !confirm {
                    return;
                }
                let ids: Vec<String> = h.sent.iter().map(|x| x.to_string()).collect();
                if let Some(l) = h.link.as_mut() {
                    l.send('V', ids.join(" ").as_bytes());
                }
            }
            'Y' => m.on_world(h, online::ITEMS, &body),
            'U' => m.on_world(h, online::POSITION, &body),
            'X' => m.on_world(h, online::PLAYER_OUT, &body),
            'E' => {
                // the relay closes after an error; the mod may join again
                h.link = None;
                m.on_world(h, online::ERROR, &body);
            }
            _ => {}
        }
    }
}

/// What the edited script does at a check.
fn check(m: &mut Mod, h: &mut Net, flag: u16, key: u16) {
    h.works.insert(0x805E, key);
    h.works.insert(0x805F, 0xA4C7);
    h.works.insert(0x8008, 1);
    h.works.insert(0x8009, 1);
    m.game.as_mut().unwrap().on_flag(h, flag);
    h.works.insert(0x805F, 0);
}

/// A check of a location of `m`'s world whose item goes to `to`.
fn check_for(m: &mut Mod, h: &mut Net, to: u32, skip: usize) -> (u32, u32) {
    let g = m.game.as_ref().unwrap();
    let (&(flag, key), locs) = g
        .checks
        .iter()
        .filter(|(_, ls)| {
            ls.len() == 1 && g.placed.get(&ls[0]).is_some_and(|&(i, r)| r == to && i < 0x1000) && !g.state.checked.contains(&ls[0])
        })
        .nth(skip)
        .expect("a location for that player");
    let loc = locs[0];
    let item = g.placed[&loc].0;
    check(m, h, flag, key);
    (loc, item)
}

#[test]
fn two_players_online() {
    let Ok(addr) = std::env::var("ARKH_METAXY") else {
        eprintln!("ARKH_METAXY not set: the online test was skipped");
        return;
    };
    let wait = Duration::from_secs(3);
    let stamp = token();
    let (na, nb, nc) = (format!("Ash{}", &stamp[..6]), format!("Misty{}", &stamp[..6]), format!("Brock{}", &stamp[..6]));

    // the owner makes and starts the world
    let mut owner = Link::open(&addr);
    let cfg = serde_json::json!({"schema": 1, "seed": 4242,
        "players": [{"name": na, "version": "black"}, {"name": nb, "version": "white"}, {"name": nc, "version": "black"}]});
    owner.send('N', format!("{V2} {}\n{}", token(), cfg).as_bytes());
    let (code, okey) = loop {
        let (t, body) = owner.next(Duration::from_secs(60)).expect("an answer to N");
        let text = String::from_utf8_lossy(&body).to_string();
        assert_ne!(t, 'E', "N refused: {text}");
        if t == 'R' && !text.starts_with("wait") {
            let mut w = text.split_whitespace();
            break (w.next().unwrap().to_string(), w.next().unwrap().to_string());
        }
    };
    owner.send('O', format!("{V2} {code} {okey} start").as_bytes());
    let (t, body) = owner.next(wait).expect("an answer to O");
    assert_eq!((t, String::from_utf8_lossy(&body).to_string()), ('O', format!("{code} started")));

    // both join from a new game (join.txt names the world)
    let mut ha = Net { addr: addr.clone(), code: *b"IRBO", ..Default::default() };
    let mut hb = Net { addr: addr.clone(), code: *b"IRAO", ..Default::default() };
    ha.files.insert(online::JOIN_FILE.into(), format!("{code} {na}").into_bytes());
    hb.files.insert(online::JOIN_FILE.into(), format!("{code} {nb}").into_bytes());
    let (mut ma, mut mb) = (Mod::default(), Mod::default());
    ma.new_game(&mut ha, 1);
    mb.new_game(&mut hb, 1);
    pump(&mut ma, &mut ha, wait);
    pump(&mut mb, &mut hb, wait);
    let ga = ma.game.as_ref().expect("A joined");
    let gb = mb.game.as_ref().expect("B joined");
    assert_eq!((ga.slot, gb.slot), (1, 2));
    assert_eq!(ga.player().version, "black");
    let key_a = ga.state.online.as_ref().unwrap().key.clone();
    assert_eq!(key_a.len(), 32);
    assert!(ha.files.keys().any(|f| f.starts_with("online-")), "the world is kept for offline starts");

    // A finds two of B's items: B gets them at once, numbered 1 and 2
    let (_, i1) = check_for(&mut ma, &mut ha, 2, 0);
    let (_, i2) = check_for(&mut ma, &mut ha, 2, 0);
    pump(&mut mb, &mut hb, wait);
    assert_eq!(mb.game.as_ref().unwrap().state.received, 2);
    assert_eq!(hb.items, vec![(i1 as u16, 1), (i2 as u16, 1)]);

    // A's own item: given at once, nothing comes back from metaxy
    let own_before = ha.items.len();
    check_for(&mut ma, &mut ha, 1, 0);
    assert_eq!(ha.items.len(), own_before + 1);

    // positions: B's zone becomes a marker on A's Town Map
    hb.link.as_mut().unwrap().send('U', b"12 3 0 7 1");
    pump(&mut ma, &mut ha, Duration::from_secs(1));
    assert_eq!(ma.names.get(&2), Some(&nb));
    assert_eq!(ha.markers.get(&2), Some(&12));

    // A saves and quits; B finds one of A's items while A is away
    ma.game.as_ref().unwrap().save(&mut ha);
    ha.world_leave();
    pump(&mut mb, &mut hb, Duration::from_millis(500));
    let (_, ia) = check_for(&mut mb, &mut hb, 1, 0);

    // A starts again: the save's world applies at once and A joins by key;
    // the push brings the item with its index
    let mut ha2 = Net { addr: addr.clone(), code: *b"IRBO", files: ha.files.clone(), state: ha.state.clone(), ..Default::default() };
    let mut ma2 = Mod::default();
    ma2.save_load(&mut ha2);
    assert!(ma2.game.is_some(), "the stored world, before any answer");
    pump(&mut ma2, &mut ha2, wait);
    let g = ma2.game.as_ref().unwrap();
    assert_eq!(g.state.online.as_ref().unwrap().key, key_a, "the same key");
    assert_eq!(g.state.received, 1);
    assert_eq!(ha2.items, vec![(ia as u16, 1)]);

    // and again with that save: the item is not given twice
    ma2.game.as_ref().unwrap().save(&mut ha2);
    ha2.world_leave();
    let mut ha3 = Net { addr: addr.clone(), code: *b"IRBO", files: ha2.files.clone(), state: ha2.state.clone(), ..Default::default() };
    let mut ma3 = Mod::default();
    ma3.save_load(&mut ha3);
    pump(&mut ma3, &mut ha3, wait);
    assert!(ha3.items.is_empty(), "nothing twice: {:?}", ha3.items);
    assert!(ha3.log.iter().all(|l| !l.contains("metaxy:")), "no error: {:?}", ha3.log);

    // a check made with no link is kept and sent on the next join
    ha3.world_leave();
    let (_, ib) = check_for(&mut ma3, &mut ha3, 2, 0);
    ma3.game.as_ref().unwrap().save(&mut ha3);
    let mut ha4 = Net { addr: addr.clone(), code: *b"IRBO", files: ha3.files.clone(), state: ha3.state.clone(), ..Default::default() };
    let mut ma4 = Mod::default();
    ma4.save_load(&mut ha4);
    pump(&mut ma4, &mut ha4, wait);
    pump(&mut mb, &mut hb, wait);
    assert_eq!(hb.items.last(), Some(&(ib as u16, 1)), "the offline check reached B");
    assert_eq!(mb.game.as_ref().unwrap().state.received, 3);

    // a new game on A's files (no save): back in by the stored key, no name-taken
    let mut ha5 = Net { addr: addr.clone(), code: *b"IRBO", files: ha4.files.clone(), ..Default::default() };
    let mut ma5 = Mod::default();
    ha4.world_leave();
    ma5.new_game(&mut ha5, 1);
    pump(&mut ma5, &mut ha5, wait);
    assert_eq!(ma5.game.as_ref().map(|g| g.slot), Some(1), "joined by key: {:?}", ha5.log);
    assert!(ha5.log.iter().all(|l| !l.contains("metaxy:")), "no error: {:?}", ha5.log);
    ha5.world_leave();
    let mut ma4 = ma5;
    let mut ha4 = ha5;
    ha4.world_join(&code, &ma4.game.as_ref().unwrap().state.online.as_ref().unwrap().key);
    pump(&mut ma4, &mut ha4, wait);

    // bad key: C joins by name and closes before confirming, so metaxy
    // releases the claim; C's next game joins with the dead key, gets
    // bad-key, joins again by name and keeps the new key
    let mut hc = Net { addr: addr.clone(), code: *b"IRBO", ..Default::default() };
    hc.files.insert(online::JOIN_FILE.into(), format!("{code} {nc}").into_bytes());
    let mut mc = Mod::default();
    mc.new_game(&mut hc, 1);
    pump_as(&mut mc, &mut hc, wait, false);
    let dead = mc.game.as_ref().expect("C joined").state.online.as_ref().unwrap().key.clone();
    hc.link = None; // closed with no V
    std::thread::sleep(Duration::from_millis(500));
    let mut hc2 = Net { addr: addr.clone(), code: *b"IRBO", files: hc.files.clone(), ..Default::default() };
    let mut mc2 = Mod::default();
    mc2.new_game(&mut hc2, 1);
    pump(&mut mc2, &mut hc2, wait);
    assert!(hc2.log.iter().any(|l| l.contains("bad-key")), "bad-key seen: {:?}", hc2.log);
    let fresh = mc2.game.as_ref().expect("C joined again").state.online.as_ref().unwrap().key.clone();
    assert_ne!(fresh, dead);
    assert!(hc2.files.values().any(|v| v == fresh.as_bytes()), "the new key is in the key file");
    hc2.world_leave();

    // both goals
    ma4.game.as_mut().unwrap().on_cleared(&mut ha4);
    mb.game.as_mut().unwrap().on_cleared(&mut hb);
    pump(&mut ma4, &mut ha4, Duration::from_millis(500));
    pump(&mut mb, &mut hb, Duration::from_millis(500));
    assert!(ma4.game.as_ref().unwrap().state.goal && mb.game.as_ref().unwrap().state.goal);
    assert!(ha4.log.iter().chain(hb.log.iter()).all(|l| !l.contains("metaxy:")), "no errors");
    ha4.world_leave();
    hb.world_leave();
    owner.send('O', format!("{V2} {code} {okey} delete").as_bytes());
}
