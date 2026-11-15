//! wasm32 entry points: the host writes a configuration (JSON) into memory
//! it got from `arkh_alloc`, calls `arkh_world_hash`, and gets the FNV-1a
//! hash of the world's JSON (the same hash `arkh-gen hash` prints), or 0 on
//! an error.

use arkh_gen::gen::{generate, Config};
use arkh_gen::rng::fnv1a;

#[no_mangle]
pub extern "C" fn arkh_alloc(len: u32) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len as usize);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` must point at `len` bytes from `arkh_alloc`.
#[no_mangle]
pub unsafe extern "C" fn arkh_world_hash(ptr: *const u8, len: u32) -> u64 {
    let bytes = std::slice::from_raw_parts(ptr, len as usize);
    let cfg: Config = match serde_json::from_slice(bytes) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let m = match arkh_gen::model() {
        Ok(m) => m,
        Err(_) => return 0,
    };
    match generate(&m, &cfg) {
        Ok(w) => fnv1a(&[&serde_json::to_vec(&w).unwrap()]),
        Err(_) => 0,
    }
}
