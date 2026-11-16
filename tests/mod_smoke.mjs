// Run the built mod (wasm) with fake eidos imports: start it, fire game.new
// through its function table (a solo world is made and stored), fire one
// flag.set with the check marker, and check that the mod gave an item.
//
// usage: node tests/mod_smoke.mjs <arkh_mod.wasm>
import { readFileSync } from "node:fs";

const bytes = readFileSync(process.argv[2]);
const handlers = {};
const files = {};
const works = { 0x805e: 0, 0x805f: 0, 0x8008: 0, 0x8009: 0 };
const given = [];
const logs = [];
let mem;
const str = (p) => {
  const b = new Uint8Array(mem.buffer);
  let e = p;
  while (b[e]) e++;
  return new TextDecoder().decode(b.subarray(p, e));
};
const eidos = {
  log: (p, n) => logs.push(new TextDecoder().decode(new Uint8Array(mem.buffer, p, n))),
  on: (kind, idx) => { handlers[kind] = idx; return 0; },
  on_frame: (idx) => { handlers.frame = idx; },
  rd32: () => 0,
  rand_next: () => 12345,
  text_language: () => 2,
  game_code: (out, size) => { new Uint8Array(mem.buffer, out, 4).set([0x49, 0x52, 0x42, 0x4f]); return 0; },
  item_add: (item, count) => { given.push([item, count]); return 0; },
  state_load: () => 0,
  state_store: () => 0,
  work_get: (id, out) => { new DataView(mem.buffer).setUint16(out, works[id] ?? 0, true); return 0; },
  work_set: (id, v) => { works[id] = v; return 0; },
  badge_set: () => 0,
  vfs_member_set: () => 0,
  vfs_member_clear: () => 0,
  vfs_member_get: () => -1, // no cartridge: apply fails, the world is still made
  script_run: (id, t) => 0,
  type_chart_set: () => 0,
  mod_file_write: (n, p, size) => { files[str(n)] = new Uint8Array(mem.buffer, p, size).slice(); return 0; },
  mod_file_read: (n, out, size, got) => { const f = files[str(n)]; if (!f) return -1; if (size) new Uint8Array(mem.buffer, out, f.length).set(f); new DataView(mem.buffer).setUint32(got, size ? f.length : 0, true); return f.length; },
  world_join: () => -1, world_checks: () => -1, world_leave: () => {}, world_read: () => -1,
  peer_position: () => 0, townmap_marker_set: () => 0, townmap_marker_clear: () => 0,
  text_get: () => -1, text_set: () => 0,
};
const { instance } = await WebAssembly.instantiate(bytes, { eidos });
mem = instance.exports.memory;
const table = instance.exports.__indirect_function_table;
if (instance.exports.eidos_mod_start() !== 0) throw new Error("start failed");
// payload copies go in a page past the module's own memory, as the port does
const page = mem.grow(1) * 65536;
const ev = (kind, size) => { const d = new DataView(mem.buffer); d.setUint32(page, size, true); d.setUint32(page + 4, kind, true); return page; };
const t0 = Date.now();
table.get(handlers[1])(ev(1, 8)); // game.new
const world = Object.keys(files).find((f) => f.startsWith("world-"));
if (!world) throw new Error("no world stored: " + logs.join(" | "));
const w = JSON.parse(new TextDecoder().decode(files[world]));
console.log(`game.new: ${world}, ${w.players[0].locations.length} locations, ${Date.now() - t0} ms`);
// one check: Route 1 woman (flag 132, key 0)
Object.assign(works, { 0x805e: 0, 0x805f: 0xa4c7, 0x8008: 17, 0x8009: 1 });
const p = ev(111, 12);
new DataView(mem.buffer).setUint16(p + 8, 132, true);
table.get(handlers[111])(p);
const loc = 0x10000 + (132 << 4);
const placed = w.players[0].locations.find((l) => l[0] === loc);
const want = placed ? placed[1] : 17;
if (given.length !== 1 || given[0][0] !== want) throw new Error(`gave ${JSON.stringify(given)}, want ${want}`);
console.log(`flag.set 132: gave item ${given[0][0]} (location ${loc})`);
console.log("logs:", logs.join(" | "));
