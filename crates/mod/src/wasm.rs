//! The eidos binding (interface version 6, sdk/api/v6/eidos_wasm.h): the
//! imports of module "eidos", the `Host` over them, the exports and the
//! event handlers. Every import used is listed in docs/v6-calls.md.

use std::cell::RefCell;

use crate::{solo_config, version_of, Game, Host};

#[link(wasm_import_module = "eidos")]
extern "C" {
    fn log(text: *const u8, len: u32);
    fn on(kind: u32, handler: u32) -> i32;
    fn on_frame(handler: u32);
    fn rd32(addr: u32) -> u32;
    fn rand_next() -> u32;
    fn text_language() -> u32;
    fn game_code(out: *mut u8, size: u32) -> i32;
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
const EVENT_SAVE_LOAD: u32 = 4;
const EVENT_FLAG_SET: u32 = 111;
const EVENT_SCRIPT_DONE: u32 = 114;
const EVENT_STARTER_CHOICES: u32 = 121;
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
}

thread_local! {
    static GAME: RefCell<Option<Game>> = const { RefCell::new(None) };
}

fn with_game(f: impl FnOnce(&mut Game, &mut Eidos)) {
    GAME.with(|g| {
        if let Ok(mut g) = g.try_borrow_mut() {
            if let Some(game) = g.as_mut() {
                f(game, &mut Eidos);
            }
        }
    });
}

fn start(game: Result<Option<Game>, String>) {
    let h = &mut Eidos;
    match game {
        Ok(Some(g)) => {
            match g.apply(h) {
                Ok(n) => h.log(&format!("arkhipelagos: world {} applied ({n} members)", g.state.world)),
                Err(e) => h.log(&format!("arkhipelagos: apply failed: {e}")),
            }
            GAME.with(|c| *c.borrow_mut() = Some(g));
        }
        Ok(None) => {
            h.member_clear_all();
            GAME.with(|c| *c.borrow_mut() = None);
        }
        Err(e) => h.log(&format!("arkhipelagos: {e}")),
    }
}

extern "C" fn on_game_new(_e: *mut Event) -> i32 {
    let h = &mut Eidos;
    let code = h.game_code();
    let seed = unsafe { ((rand_next() as u64) << 32) | rand_next() as u64 };
    let cfg = solo_config(version_of(&code), seed, "Player", Default::default());
    start(Game::create_solo(h, &cfg).map(Some));
    CONTINUE
}

extern "C" fn on_save_load(_e: *mut Event) -> i32 {
    start(Game::load(&mut Eidos));
    CONTINUE
}

extern "C" fn on_save_begin(_e: *mut Event) -> i32 {
    with_game(|g, h| g.save(h));
    CONTINUE
}

extern "C" fn on_flag_set(e: *mut FlagSet) -> i32 {
    let flag = unsafe { (*e).flag };
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

extern "C" fn on_tick() {
    with_game(|g, h| g.tick(h));
}

#[no_mangle]
pub extern "C" fn eidos_mod_start() -> i32 {
    unsafe {
        let ok = on(EVENT_GAME_NEW, on_game_new as *const () as usize as u32) == 0
            && on(EVENT_SAVE_LOAD, on_save_load as *const () as usize as u32) == 0
            && on(EVENT_SAVE_BEGIN, on_save_begin as *const () as usize as u32) == 0
            && on(EVENT_FLAG_SET, on_flag_set as *const () as usize as u32) == 0
            && on(EVENT_SCRIPT_DONE, on_script_done as *const () as usize as u32) == 0
            && on(EVENT_STARTER_CHOICES, on_starter_choices as *const () as usize as u32) == 0;
        on_frame(on_tick as *const () as usize as u32);
        if ok {
            0
        } else {
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn eidos_mod_stop() {
    GAME.with(|c| *c.borrow_mut() = None);
}
