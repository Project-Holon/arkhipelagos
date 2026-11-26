//! The eidos binding (interface version 6, sdk/api/v6/eidos_wasm.h): the
//! imports of module "eidos", the `Host` over them, the exports and the
//! event handlers. Every import used is listed in docs/v6-calls.md.

use std::cell::RefCell;

use crate::online::Mod;
use crate::{Game, Host};

#[link(wasm_import_module = "eidos")]
extern "C" {
    fn log(text: *const u8, len: u32);
    fn on(kind: u32, handler: u32) -> i32;
    fn on_frame(handler: u32);
    fn rd32(addr: u32) -> u32;
    fn time_utc() -> i64;
    fn ticks_ms() -> u64;
    fn text_language() -> u32;
    fn game_code(out: *mut u8, size: u32) -> i32;
    fn world_join(code: *const u8, name_or_key: *const u8, ticket: *mut u32) -> i32;
    fn world_checks(ids: *const u32, count: u32) -> i32;
    fn world_leave();
    fn world_read(out: *mut u8, size: u32) -> i32;
    fn peer_position(on: i32) -> i32;
    fn townmap_marker_set(id: u32, zone: u32, icon: u32, name: *const u8) -> i32;
    fn townmap_marker_clear(id: u32) -> i32;
    fn text_get(archive: u32, file: u32, entry: u32, block: u32, out: *mut u16, max_units: u32) -> i32;
    fn text_set(archive: u32, file: u32, entry: u32, block: u32, units: *const u16, count: u32) -> i32;
    fn item_add(item: u32, count: u32) -> i32;
    fn state_load(buf: *mut u8, size: u32) -> i32;
    fn state_store(buf: *const u8, size: u32) -> i32;
    fn work_get(id: u32, out: *mut u16) -> i32;
    fn dex_caught(species: u32) -> i32;
    fn work_set(id: u32, value: u32) -> i32;
    fn badge_set(bit: u32) -> i32;
    fn vfs_member_set(archive: *const u8, index: u32, data: *const u8, size: u32) -> i32;
    fn vfs_member_clear(archive: *const u8, index: u32) -> i32;
    fn vfs_member_get(archive: *const u8, index: u32, out: *mut u8, size: u32) -> i32;
    fn script_run(script_id: u32, ticket: *mut u32) -> i32;
    fn type_chart_set(chart: *const u8) -> i32;
    fn mod_file_write(name: *const u8, data: *const u8, size: u32) -> i32;
    fn mod_file_read(name: *const u8, out: *mut u8, size: u32, got: *mut u32) -> i32;
    fn mode_get(out: *mut u8, size: u32) -> i32;
    fn mode_code(out: *mut u8, size: u32) -> i32;
    fn setting_get(key: *const u8) -> i32;
    fn setting_text(key: *const u8, out: *mut u8, size: u32) -> i32;
    fn world_create(config_json: *const u8, ticket: *mut u32) -> i32;
    fn player_name(out: *mut u8, size: u32) -> i32;
    fn world_owner(code: *const u8, owner_key: *const u8, action: u32) -> i32;
    fn ap_link(room: *const u8, slot: *const u8, password: *const u8, ticket: *mut u32) -> i32;
    fn ap_send(json: *const u8, size: u32) -> i32;
    fn ap_read(out: *mut u8, size: u32) -> i32;
    fn tracker_set(text: *const u8, size: u32) -> i32;
}

// event kinds (sdk/api/v6/eidos_wasm.h)
const EVENT_GAME_NEW: u32 = 1;
const EVENT_SAVE_BEGIN: u32 = 2;
const EVENT_GAME_CLEARED: u32 = 5;
const EVENT_SAVE_LOAD: u32 = 4;
const EVENT_FLAG_SET: u32 = 111;
const EVENT_SCRIPT_DONE: u32 = 114;
const EVENT_STARTER_CHOICES: u32 = 121;
const EVENT_WORLD: u32 = 124;
const EVENT_AP: u32 = 125;
const EVENT_BATTLE_END: u32 = 80;
const EVENT_DEX_REGISTER: u32 = 112;
const CONTINUE: i32 = 0;

/// The DS addresses of the cartridge header's game code (the header copies
/// the firmware leaves in main memory): only used if `game_code` fails.
const GAME_CODE_ADDRS: [u32; 2] = [0x027F_FE0C, 0x02FF_FE0C];

#[repr(C)]
struct Event {
    size: u32,
    kind: u32,
}

#[repr(C)]
struct FlagSet {
    h: Event,
    flag: u16,
    value: u8,
}

#[repr(C)]
struct ScriptDone {
    h: Event,
    ticket: u32,
    script_id: u16,
    pad: u16,
}

#[repr(C)]
struct WorldEvent {
    h: Event,
    kind: u32,
    size: u32,
    ticket: u32,
    player: u32,
}

/// `ap` (BwApEvent)
#[repr(C)]
struct ApEvent {
    h: Event,
    kind: u32,
    size: u32,
    ticket: u32,
}

#[repr(C)]
struct StarterChoices {
    h: Event,
    species: [u16; 3],
    pad: u16,
}

fn cstr(s: &str) -> Vec<u8> {
    let mut v = s.as_bytes().to_vec();
    v.push(0);
    v
}

struct Eidos;

impl Host for Eidos {
    fn log(&mut self, text: &str) {
        unsafe { log(text.as_ptr(), text.len() as u32) }
    }
    fn member_get(&mut self, archive: &str, index: u32) -> Option<Vec<u8>> {
        let a = cstr(archive);
        let n = unsafe { vfs_member_get(a.as_ptr(), index, std::ptr::null_mut(), 0) };
        if n < 0 {
            return None;
        }
        let mut v = vec![0u8; n as usize];
        let m = unsafe { vfs_member_get(a.as_ptr(), index, v.as_mut_ptr(), n as u32) };
        (m == n).then_some(v)
    }
    fn member_set(&mut self, archive: &str, index: u32, data: &[u8]) -> bool {
        let a = cstr(archive);
        unsafe { vfs_member_set(a.as_ptr(), index, data.as_ptr(), data.len() as u32) == 0 }
    }
    fn member_clear_all(&mut self) {
        let a = cstr("");
        unsafe { vfs_member_clear(a.as_ptr(), 0xFFFF_FFFF) };
    }
    fn type_chart_set(&mut self, chart: &[u8]) -> bool {
        chart.len() == 289 && unsafe { type_chart_set(chart.as_ptr()) == 0 }
    }
    fn work_get(&mut self, id: u16) -> Option<u16> {
        let mut v = 0u16;
        (unsafe { work_get(id as u32, &mut v) } == 0).then_some(v)
    }
    fn dex_caught(&mut self, species: u16) -> Option<bool> {
        let r = unsafe { dex_caught(species as u32) };
        (r >= 0).then_some(r > 0)
    }
    fn work_set(&mut self, id: u16, value: u16) -> bool {
        unsafe { work_set(id as u32, value as u32) == 0 }
    }
    fn item_add(&mut self, item: u16, count: u16) -> bool {
        unsafe { item_add(item as u32, count as u32) == 0 }
    }
    fn badge_set(&mut self, bit: u32) -> bool {
        unsafe { badge_set(bit) == 0 }
    }
    fn script_run(&mut self, id: u16) -> Option<u32> {
        let mut t = 0u32;
        (unsafe { script_run(id as u32, &mut t) } == 0).then_some(t)
    }
    fn state_load(&mut self) -> Option<Vec<u8>> {
        let mut v = vec![0u8; 256 * 1024];
        let n = unsafe { state_load(v.as_mut_ptr(), v.len() as u32) };
        if n <= 0 {
            return None;
        }
        v.truncate(n as usize);
        Some(v)
    }
    fn state_store(&mut self, data: &[u8]) -> bool {
        unsafe { state_store(data.as_ptr(), data.len() as u32) == 0 }
    }
    fn file_read(&mut self, name: &str) -> Option<Vec<u8>> {
        let n = cstr(name);
        let mut got = 0u32;
        let size = unsafe { mod_file_read(n.as_ptr(), std::ptr::null_mut(), 0, &mut got) };
        if size < 0 {
            return None;
        }
        let mut v = vec![0u8; size as usize];
        let r = unsafe { mod_file_read(n.as_ptr(), v.as_mut_ptr(), size as u32, &mut got) };
        (r == size && got as i32 == size).then_some(v)
    }
    fn file_write(&mut self, name: &str, data: &[u8]) -> bool {
        let n = cstr(name);
        unsafe { mod_file_write(n.as_ptr(), data.as_ptr(), data.len() as u32) == 0 }
    }
    fn game_code(&mut self) -> [u8; 4] {
        let mut c = [0u8; 5];
        if unsafe { game_code(c.as_mut_ptr(), c.len() as u32) } == 0 {
            return [c[0], c[1], c[2], c[3]];
        }
        for a in GAME_CODE_ADDRS {
            let c = unsafe { rd32(a) }.to_le_bytes();
            if c[0] == b'I' && c[1] == b'R' && (c[2] == b'B' || c[2] == b'A') {
                return c;
            }
        }
        // no header copy: the language from the text, Black assumed
        let lang = unsafe { text_language() };
        let l = match lang {
            1 => b'J',
            3 => b'F',
            4 => b'I',
            5 => b'D',
            7 => b'S',
            _ => b'O',
        };
        self.log("arkhipelagos: no game code in memory; Black assumed");
        [b'I', b'R', b'B', l]
    }
    fn world_join(&mut self, code: &str, name_or_key: &str) -> bool {
        let (c, n) = (cstr(code), cstr(name_or_key));
        let mut t = 0u32;
        unsafe { world_join(c.as_ptr(), n.as_ptr(), &mut t) == 0 }
    }
    fn world_checks(&mut self, ids: &[u32]) -> bool {
        // the port sends them in as many V frames as it needs
        unsafe { world_checks(ids.as_ptr(), ids.len() as u32) == 0 }
    }
    fn world_leave(&mut self) {
        unsafe { world_leave() }
    }
    fn peer_position(&mut self, on: bool) {
        unsafe { peer_position(on as i32) };
    }
    fn townmap_marker_set(&mut self, id: u32, zone: u32, name: &str) {
        let n = cstr(name);
        unsafe { townmap_marker_set(id, zone, 0, n.as_ptr()) };
    }
    fn townmap_marker_clear(&mut self, id: u32) {
        unsafe { townmap_marker_clear(id) };
    }
    fn text_get(&mut self, archive: u32, file: u32, entry: u32, block: u32) -> Option<Vec<u16>> {
        let mut v = vec![0u16; 512];
        let n = unsafe { text_get(archive, file, entry, block, v.as_mut_ptr(), v.len() as u32) };
        if n < 0 {
            return None;
        }
        v.truncate((n as usize).min(512));
        Some(v)
    }
    fn text_set(&mut self, archive: u32, file: u32, entry: u32, block: u32, units: &[u16]) -> bool {
        unsafe { text_set(archive, file, entry, block, units.as_ptr(), units.len() as u32) == 0 }
    }
    fn mode(&mut self) -> Option<String> {
        let mut b = [0u8; 64];
        let n = unsafe { mode_get(b.as_mut_ptr(), b.len() as u32) };
        (n >= 0).then(|| String::from_utf8_lossy(&b[..(n as usize).min(63)]).to_string())
    }
    fn mode_code(&mut self) -> Option<String> {
        // a world code, or an Archipelago room's line (up to 160 bytes)
        let mut b = [0u8; 256];
        let n = unsafe { mode_code(b.as_mut_ptr(), b.len() as u32) };
        (n >= 0).then(|| String::from_utf8_lossy(&b[..(n as usize).min(255)]).to_string())
    }
    fn setting(&mut self, key: &str) -> Option<i32> {
        let k = cstr(key);
        let v = unsafe { setting_get(k.as_ptr()) };
        (v >= 0).then_some(v)
    }
    fn setting_text(&mut self, key: &str) -> Option<String> {
        let k = cstr(key);
        let mut b = [0u8; 128];
        let n = unsafe { setting_text(k.as_ptr(), b.as_mut_ptr(), b.len() as u32) };
        (n >= 0).then(|| String::from_utf8_lossy(&b[..(n as usize).min(127)]).to_string())
    }
    fn player_name(&mut self) -> Option<String> {
        let mut b = [0u8; 64];
        let n = unsafe { player_name(b.as_mut_ptr(), b.len() as u32) };
        (n > 0).then(|| String::from_utf8_lossy(&b[..(n as usize).min(63)]).to_string())
    }
    fn world_start(&mut self, code: &str, owner_key: &str) -> bool {
        let (c, k) = (cstr(code), cstr(owner_key));
        unsafe { world_owner(c.as_ptr(), k.as_ptr(), 0) == 0 }
    }
    fn world_create(&mut self, config_json: &str) -> bool {
        let c = cstr(config_json);
        let mut t = 0u32;
        unsafe { world_create(c.as_ptr(), &mut t) == 0 }
    }
    fn ap_link(&mut self, room: &str, slot: &str, password: &str) -> bool {
        let (r, s, p) = (cstr(room), cstr(slot), cstr(password));
        let mut t = 0u32;
        unsafe { ap_link(r.as_ptr(), s.as_ptr(), p.as_ptr(), &mut t) == 0 }
    }
    fn ap_send(&mut self, json: &str) -> bool {
        unsafe { ap_send(json.as_ptr(), json.len() as u32) == 0 }
    }
    fn tracker_set(&mut self, text: &str) -> bool {
        unsafe { tracker_set(text.as_ptr(), text.len() as u32) == 0 }
    }
}

thread_local! {
    static MOD: RefCell<Mod> = RefCell::new(Mod::default());
}

fn with_mod(f: impl FnOnce(&mut Mod, &mut Eidos)) {
    MOD.with(|m| match m.try_borrow_mut() {
        Ok(mut m) => f(&mut m, &mut Eidos),
        Err(_) => Eidos.log("arkhipelagos: an event came while another was handled; skipped"),
    });
}

fn with_game(f: impl FnOnce(&mut Game, &mut Eidos)) {
    with_mod(|m, h| {
        if let Some(g) = m.game.as_mut() {
            f(g, h);
        }
    });
}

extern "C" fn on_game_new(_e: *mut Event) -> i32 {
    // eidos's rand_next starts every run at the same fixed seed until a mod
    // seeds it (sdk/mod_timer.c), so it gave every new world the same seed.
    // The clock differs from game to game: the seconds since 1970 and the
    // milliseconds since eidos started, mixed by SplitMix64's finalizer.
    let seed = unsafe { mix((time_utc() as u64) ^ ticks_ms().rotate_left(32)) };
    with_mod(|m, h| m.new_game(h, seed));
    CONTINUE
}

/// SplitMix64's output step (Steele, Lea and Flood 2014): every input bit
/// reaches every output bit.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

extern "C" fn on_save_load(_e: *mut Event) -> i32 {
    Eidos.log("arkhipelagos: save.load");
    with_mod(|m, h| m.save_load(h));
    CONTINUE
}

extern "C" fn on_save_begin(_e: *mut Event) -> i32 {
    with_mod(|m, h| m.save(h));
    CONTINUE
}

extern "C" fn on_game_cleared(_e: *mut Event) -> i32 {
    with_game(|g, h| g.on_cleared(h));
    CONTINUE
}

extern "C" fn on_flag_set(e: *mut FlagSet) -> i32 {
    let flag = unsafe { (*e).flag };
    if std::option_env!("ARKH_TRACE_FLAGS").is_some() {
        let mark = Eidos.work_get(0x805F);
        Eidos.log(&format!("arkhipelagos: flag.set {flag}, work 0x805f {mark:?}"));
    }
    with_game(|g, h| {
        g.on_flag(h, flag);
    });
    CONTINUE
}

/// `dex.register` (BwDexRegister).
#[repr(C)]
struct DexRegister {
    h: Event,
    species: u16,
    caught: u8,
}

/// `battle.end` (BwBattleEnd).
#[repr(C)]
struct BattleEnd {
    h: Event,
    outcome: u8,
    wild: u8,
    trainer_id: [u16; 2],
    turns: u16,
}

extern "C" fn on_dex_register(e: *mut DexRegister) -> i32 {
    let (species, caught) = unsafe { ((*e).species, (*e).caught != 0) };
    with_game(|g, h| g.on_dex_register(h, species, caught));
    CONTINUE
}

extern "C" fn on_battle_end(e: *mut BattleEnd) -> i32 {
    let (outcome, wild, trainers) = unsafe { ((*e).outcome, (*e).wild, (*e).trainer_id) };
    if wild == 0 {
        with_game(|g, h| g.on_battle_end(h, outcome, trainers));
    }
    CONTINUE
}

extern "C" fn on_script_done(e: *mut ScriptDone) -> i32 {
    let id = unsafe { (*e).script_id };
    // both message scripts: the game's "obtained X" and the mod's lines
    if id == crate::MSG_SCRIPT || id == crate::LINE_SCRIPT {
        with_game(|g, h| g.script_done(h));
    }
    CONTINUE
}

extern "C" fn on_starter_choices(e: *mut StarterChoices) -> i32 {
    with_game(|g, _| {
        let s = &g.player().starters;
        if s.len() == 3 {
            unsafe {
                for (i, sp) in s.iter().enumerate() {
                    (*e).species[i] = *sp as u16;
                }
            }
        }
    });
    CONTINUE
}

extern "C" fn on_world(e: *mut WorldEvent) -> i32 {
    let (kind, size) = unsafe { ((*e).kind, (*e).size) };
    let mut body = vec![0u8; size as usize];
    let n = unsafe { world_read(body.as_mut_ptr(), size) };
    if n >= 0 {
        body.truncate(n as usize);
        with_mod(|m, h| m.on_world(h, kind, &body));
    }
    CONTINUE
}

/// The event `ap`: one Archipelago message (up to 2 MB, read while the
/// handler runs), the link's answer, an error, or its end.
extern "C" fn on_ap(e: *mut ApEvent) -> i32 {
    let (kind, size) = unsafe { ((*e).kind, (*e).size) };
    let mut body = vec![0u8; size as usize];
    let mut n = unsafe { ap_read(body.as_mut_ptr(), size) };
    if n > size as i32 {
        // more than the payload said: read it whole
        body = vec![0u8; n as usize];
        n = unsafe { ap_read(body.as_mut_ptr(), n as u32) };
    }
    if n >= 0 {
        body.truncate(n as usize);
    } else {
        body.clear();
    }
    with_mod(|m, h| m.on_ap(h, kind, &body));
    CONTINUE
}

extern "C" fn on_tick() {
    with_mod(|m, h| m.tick(h));
}

#[no_mangle]
pub extern "C" fn eidos_mod_start() -> i32 {
    let binds: [(u32, u32, &str); 11] = [
        (EVENT_GAME_NEW, on_game_new as *const () as usize as u32, "game.new"),
        (EVENT_SAVE_LOAD, on_save_load as *const () as usize as u32, "save.load"),
        (EVENT_SAVE_BEGIN, on_save_begin as *const () as usize as u32, "save.begin"),
        (EVENT_FLAG_SET, on_flag_set as *const () as usize as u32, "flag.set"),
        (EVENT_SCRIPT_DONE, on_script_done as *const () as usize as u32, "script.done"),
        (EVENT_STARTER_CHOICES, on_starter_choices as *const () as usize as u32, "starter.choices"),
        (EVENT_GAME_CLEARED, on_game_cleared as *const () as usize as u32, "game.cleared"),
        (EVENT_WORLD, on_world as *const () as usize as u32, "world"),
        (EVENT_DEX_REGISTER, on_dex_register as *const () as usize as u32, "dex.register"),
        (EVENT_BATTLE_END, on_battle_end as *const () as usize as u32, "battle.end"),
        (EVENT_AP, on_ap as *const () as usize as u32, "ap"),
    ];
    // a bind that fails is logged and the others still work
    let mut failed = Vec::new();
    for (kind, f, name) in binds {
        if unsafe { on(kind, f) } != 0 {
            failed.push(name);
        }
    }
    unsafe { on_frame(on_tick as *const () as usize as u32) };
    let h = &mut Eidos;
    if failed.is_empty() {
        h.log("arkhipelagos: started");
    } else {
        h.log(&format!("arkhipelagos: started; these events could not be bound: {failed:?}"));
    }
    0
}

#[no_mangle]
pub extern "C" fn eidos_mod_stop() {
    MOD.with(|c| *c.borrow_mut() = Mod::default());
}
