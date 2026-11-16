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
    fn rand_next() -> u32;
    fn text_language() -> u32;
    fn game_code(out: *mut u8, size: u32) -> i32;
    fn world_join(code: *const u8, name_or_key: *const u8, ticket: *mut u32) -> i32;
    fn world_checks(ids: *const u32, count: u32) -> i32;
    fn world_leave();
    fn world_read(out: *mut u8, size: u32) -> i32;
    fn peer_position(on: i32) -> i32;
    fn townmap_marker_set(id: u32, zone: u32, icon: u32, name: *const u8) -> i32;
    fn townmap_marker_clear(id: u32) -> i32;
    fn item_add(item: u32, count: u32) -> i32;
    fn state_load(buf: *mut u8, size: u32) -> i32;
    fn state_store(buf: *const u8, size: u32) -> i32;
    fn work_get(id: u32, out: *mut u16) -> i32;
    fn work_set(id: u32, value: u32) -> i32;
    fn badge_set(bit: u32) -> i32;
    fn vfs_member_set(archive: *const u8, index: u32, data: *const u8, size: u32) -> i32;
    fn vfs_member_clear(archive: *const u8, index: u32) -> i32;
    fn vfs_member_get(archive: *const u8, index: u32, out: *mut u8, size: u32) -> i32;
    fn script_run(script_id: u32, ticket: *mut u32) -> i32;
    fn type_chart_set(chart: *const u8) -> i32;
    fn mod_file_write(name: *const u8, data: *const u8, size: u32) -> i32;
    fn mod_file_read(name: *const u8, out: *mut u8, size: u32, got: *mut u32) -> i32;
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
    let seed = unsafe { ((rand_next() as u64) << 32) | rand_next() as u64 };
    with_mod(|m, h| m.new_game(h, seed));
    CONTINUE
}

extern "C" fn on_save_load(_e: *mut Event) -> i32 {
    Eidos.log("arkhipelagos: save.load");
    with_mod(|m, h| m.save_load(h));
    CONTINUE
}

extern "C" fn on_save_begin(_e: *mut Event) -> i32 {
    with_game(|g, h| g.save(h));
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

extern "C" fn on_script_done(e: *mut ScriptDone) -> i32 {
    let id = unsafe { (*e).script_id };
    if id == crate::MSG_SCRIPT {
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

extern "C" fn on_tick() {
    with_game(|g, h| g.tick(h));
}

#[no_mangle]
pub extern "C" fn eidos_mod_start() -> i32 {
    let binds: [(u32, u32, &str); 8] = [
        (EVENT_GAME_NEW, on_game_new as *const () as usize as u32, "game.new"),
        (EVENT_SAVE_LOAD, on_save_load as *const () as usize as u32, "save.load"),
        (EVENT_SAVE_BEGIN, on_save_begin as *const () as usize as u32, "save.begin"),
        (EVENT_FLAG_SET, on_flag_set as *const () as usize as u32, "flag.set"),
        (EVENT_SCRIPT_DONE, on_script_done as *const () as usize as u32, "script.done"),
        (EVENT_STARTER_CHOICES, on_starter_choices as *const () as usize as u32, "starter.choices"),
        (EVENT_GAME_CLEARED, on_game_cleared as *const () as usize as u32, "game.cleared"),
        (EVENT_WORLD, on_world as *const () as usize as u32, "world"),
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
