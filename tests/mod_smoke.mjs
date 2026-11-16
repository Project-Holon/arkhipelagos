// Run the built mod (wasm) with fake eidos imports.
//
// Solo: start it, fire game.new through its function table (a solo world
// is made and stored), fire one flag.set with the check marker, and check
// that the mod gave an item.
//
// Archipelago: a new game in mode archipelago with a room line in
// mode_code; the fake ap_link, ap_send and ap_read play the room's side
// with messages recorded from Archipelago 0.6.8 (crates/mod/tests/ap):
// LINKED, RoomInfo (the mod sends Connect), Connected with a real
// slot_data and the items already found for this slot, one more item, a
// check, and the goal.
//
// usage: node tests/mod_smoke.mjs <arkh_mod.wasm>
import { readFileSync } from "node:fs";

const bytes = readFileSync(process.argv[2]);
const fixture = (n) => readFileSync(new URL(`../crates/mod/tests/ap/${n}`, import.meta.url), "utf8");

async function start(opts) {
  const s = {
    handlers: {},
    files: {},
    works: { 0x805e: 0, 0x805f: 0, 0x8008: 0, 0x8009: 0 },
    given: [],
    logs: [],
    links: [],
    sent: [],
    reading: null, // the bytes ap_read gives while a handler runs
    texts: [],
    scripts: [],
  };
  let mem;
  const str = (p) => {
    const b = new Uint8Array(mem.buffer);
    let e = p;
    while (b[e]) e++;
    return new TextDecoder().decode(b.subarray(p, e));
  };
  const put = (out, size, text) => {
    const b = new TextEncoder().encode(text);
    new Uint8Array(mem.buffer, out, Math.min(size, b.length)).set(b.subarray(0, size));
    return b.length;
  };
  const eidos = {
    log: (p, n) => s.logs.push(new TextDecoder().decode(new Uint8Array(mem.buffer, p, n))),
    on: (kind, idx) => { s.handlers[kind] = idx; return 0; },
    on_frame: (idx) => { s.handlers.frame = idx; },
    rd32: () => 0,
    rand_next: () => 12345,
    text_language: () => 2,
    game_code: (out, size) => { put(out, size, opts.code ?? "IRBO"); return 0; },
    item_add: (item, count) => { s.given.push([item, count]); return 0; },
    state_load: () => 0,
    state_store: () => 0,
    work_get: (id, out) => { new DataView(mem.buffer).setUint16(out, s.works[id] ?? 0, true); return 0; },
    work_set: (id, v) => { s.works[id] = v; return 0; },
    badge_set: () => 0,
    vfs_member_set: () => 0,
    vfs_member_clear: () => 0,
    // no cartridge: apply fails, the world is still made; 862's text bank
    // header for the line entry (1 block, 15 entries)
    vfs_member_get: (a, i, out, size) => {
      if (str(a) !== "a/0/0/3" || i !== 283) return -1;
      if (size >= 8) new Uint8Array(mem.buffer, out, 8).set([1, 0, 15, 0, 0, 0, 0, 0]);
      return 8;
    },
    script_run: (id, t) => { s.scripts.push(id); return 0; },
    type_chart_set: () => 0,
    mod_file_write: (n, p, size) => { s.files[str(n)] = new Uint8Array(mem.buffer, p, size).slice(); return 0; },
    mod_file_read: (n, out, size, got) => { const f = s.files[str(n)]; if (!f) return -1; if (size) new Uint8Array(mem.buffer, out, f.length).set(f); new DataView(mem.buffer).setUint32(got, size ? f.length : 0, true); return f.length; },
    world_join: () => -1, world_checks: () => -1, world_leave: () => {}, world_read: () => -1,
    peer_position: () => 0, townmap_marker_set: () => 0, townmap_marker_clear: () => 0,
    text_get: () => -1,
    text_set: (a, f, e, b, units, count) => {
      s.texts.push(String.fromCharCode(...new Uint16Array(mem.buffer.slice(units, units + count * 2)).filter((u) => u < 0xf000)));
      return 0;
    },
    // the Multiplayer screen's services: the mode the new game came from
    mode_get: (out, size) => put(out, size, opts.mode),
    mode_code: (out, size) => put(out, size, opts.modeCode ?? ""),
    setting_get: () => -1, setting_text: () => -1,
    player_name: () => 0, world_create: () => -1, world_owner: () => -1,
    dex_caught: () => 0,
    ap_link: (room, slot, pass, ticket) => { s.links.push([str(room), str(slot), str(pass)]); new DataView(mem.buffer).setUint32(ticket, 1, true); return 0; },
    ap_send: (p, size) => { s.sent.push(...JSON.parse(new TextDecoder().decode(new Uint8Array(mem.buffer, p, size)))); return 0; },
    ap_read: (out, size) => {
      if (s.reading === null) return -1;
      new Uint8Array(mem.buffer, out, Math.min(size, s.reading.length)).set(s.reading.subarray(0, size));
      return s.reading.length;
    },
  };
  const { instance } = await WebAssembly.instantiate(bytes, { eidos });
  mem = instance.exports.memory;
  const table = instance.exports.__indirect_function_table;
  if (instance.exports.eidos_mod_start() !== 0) throw new Error("start failed");
  // payload copies go in a page past the module's own memory, as the port does
  const page = mem.grow(1) * 65536;
  s.ev = (kind, size) => { const d = new DataView(mem.buffer); d.setUint32(page, size, true); d.setUint32(page + 4, kind, true); return page; };
  s.fire = (kind, size, fill) => { const p = s.ev(kind, size); if (fill) fill(new DataView(mem.buffer), p); table.get(s.handlers[kind])(p); };
  s.frame = () => table.get(s.handlers.frame)();
  // the `ap` event (125): kind, size, ticket; the bytes through ap_read
  s.ap = (kind, text) => {
    s.reading = new TextEncoder().encode(text);
    s.fire(125, 20, (d, p) => { d.setUint32(p + 8, kind, true); d.setUint32(p + 12, s.reading.length, true); d.setUint32(p + 16, 1, true); });
    s.reading = null;
  };
  // flag.set (111) with the check marker
  s.check = (flag, key, item) => {
    Object.assign(s.works, { 0x805e: key, 0x805f: 0xa4c7, 0x8008: item, 0x8009: 1 });
    s.fire(111, 12, (d, p) => d.setUint16(p + 8, flag, true));
    s.works[0x805f] = 0;
  };
  // script.done (114) for each message script run since the last call
  s.done = () => {
    const ran = s.scripts.splice(0);
    for (const id of ran) s.fire(114, 16, (d, p) => { d.setUint32(p + 8, 1, true); d.setUint16(p + 12, id, true); });
    return ran;
  };
  return s;
}

// ---- solo
{
  const s = await start({ mode: "solo" });
  const t0 = Date.now();
  s.fire(1, 8); // game.new
  const world = Object.keys(s.files).find((f) => f.startsWith("world-"));
  if (!world) throw new Error("no world stored: " + s.logs.join(" | "));
  const w = JSON.parse(new TextDecoder().decode(s.files[world]));
  console.log(`game.new: ${world}, ${w.players[0].locations.length} locations, ${Date.now() - t0} ms`);
  // one check: Route 1 woman (flag 132, key 0)
  s.check(132, 0, 17);
  const loc = 0x10000 + (132 << 4);
  const placed = w.players[0].locations.find((l) => l[0] === loc);
  const want = placed ? placed[1] : 17;
  if (s.given.length !== 1 || s.given[0][0] !== want) throw new Error(`gave ${JSON.stringify(s.given)}, want ${want}`);
  console.log(`flag.set 132: gave item ${s.given[0][0]} (location ${loc})`);
  console.log("logs:", s.logs.join(" | "));
}

// ---- Archipelago
{
  const s = await start({ mode: "archipelago", modeCode: "ap 127.0.0.1:38281 Ash -" });
  const fail = (what) => { throw new Error(`archipelago: ${what}; logs: ${s.logs.join(" | ")}`); };
  s.fire(1, 8); // game.new
  if (JSON.stringify(s.links) !== JSON.stringify([["127.0.0.1:38281", "Ash", ""]])) fail(`ap_link ${JSON.stringify(s.links)}`);
  s.ap(0, "ok"); // LINKED
  s.ap(1, fixture("room_info.json"));
  const connect = s.sent.find((c) => c.cmd === "Connect");
  if (!connect || connect.game !== "Pokemon Black (arkhipelagos)" || connect.name !== "Ash" || connect.items_handling !== 1 || connect.slot_data !== true) fail(`Connect ${JSON.stringify(connect)}`);
  // Connected (a real slot_data) and the two items Quest found for Ash
  s.ap(1, fixture("connected.json"));
  if (!s.files["ap-35931773795037525048-1.json"]) fail("no world file");
  if (JSON.stringify(s.given) !== "[[251,1],[459,1]]") fail(`gave ${JSON.stringify(s.given)}`);
  const scouts = s.sent.find((c) => c.cmd === "LocationScouts");
  if (!scouts || scouts.locations.length !== 6) fail("no scouts");
  s.ap(1, fixture("location_info.json"));
  if (!s.sent.some((c) => c.cmd === "GetDataPackage" && c.games[0] === "APQuest")) fail("no GetDataPackage");
  s.ap(1, fixture("data_package.json"));
  // the sent-you lines, one after the other (each line script's end
  // starts the next)
  s.frame();
  for (let i = 0; i < 4 && s.done().length; i++);
  if (s.texts.length !== 2 || !s.texts.every((t) => t.startsWith("Quest sent you"))) fail(`lines ${JSON.stringify(s.texts)}`);
  // one more item, by index
  s.ap(1, JSON.stringify([{ cmd: "ReceivedItems", index: 2, items: [{ item: 280, location: 5, player: 2, flags: 0 }] }]));
  if (s.given.length !== 3 || s.given[2][0] !== 280) fail(`gave ${JSON.stringify(s.given)}`);
  s.frame();
  s.done();
  // a check of a location holding APQuest's Shield (Opelucid Gym's guide,
  // flag 125, key 0): sent to the room, a found-for line
  s.check(125, 0, 1);
  const checks = s.sent.filter((c) => c.cmd === "LocationChecks");
  if (checks.length !== 1 || JSON.stringify(checks[0].locations) !== "[67536]") fail(`checks ${JSON.stringify(checks)}`);
  s.frame();
  s.done();
  if (s.texts[s.texts.length - 1] !== "You found Shieldfor Quest!") fail(`lines ${JSON.stringify(s.texts)}`);
  // the goal (goal ghetsis: the first ending)
  s.fire(5, 8);
  if (!s.sent.some((c) => c.cmd === "StatusUpdate" && c.status === 30)) fail("no goal");
  console.log(`archipelago: linked, connected, ${scouts.locations.length} foreign items named, ${s.given.length} items received, a check and the goal sent`);
}
