# The eidos API calls the arkhipelagos mod uses

The mod is a wasm code mod (`crates/mod`, built into `mod/arkhipelagos.wasm`
by `tools/build_mod.sh`; manifest `mod/eidos_mod.txt`, `api = 7`: the clock calls are version 7). Item
numbers point to `docs/eidos-api-proposal.md` (approved, interface
version 6). Names and signatures are those of eidos's generated header
`sdk/api/v6/eidos_wasm.h`; `tools/wasm_imports.py` checks the built
module against it (`EIDOS_SDK=<eidos>/sdk sh tests/check.sh`).

## What the module imports now (solo, online and Archipelago)

42 imports, all from module `eidos`:

| Import | Version | Proposal | What for |
|---|---|---|---|
| `vfs_member_get` | 6 | item 1 | the player's own members, to build on (called with size 0 first: it returns the whole size) |
| `vfs_member_set` | 6 | item 1 | the changed members: scripts (a/0/5/7), personal, learnsets, evolutions, moves, encounters, trainer parties |
| `vfs_member_clear` | 6 | item 1 | `("", 0xFFFFFFFF)`, all of the mod's members, before each apply and for a save with no world |
| `type_chart_set` | 6 | item 11 | the world's type chart, 289 bytes, `cell[atk * 17 + def]` |
| `work_get` | 6 | item 3 | inside `flag.set`: works 0x805F (marker), 0x805E (key), 0x8008 and 0x8009 (original item and count) |
| `work_set` | 6 | item 3 | saved works 0x40FE and 0x40FF (the item and count the message scripts show) and 0x40FD (the line's entry) |
| `badge_set` | 6 | item 4 | a badge received as an item, or a badge location that is not in the world |
| `script_run` | 6 | item 5 | the message scripts: 2815 (the game's "obtained X" after an item this player found) and 2816 (one of the mod's lines) |
| `text_set`, `text_get` | 6 | item 2 | the mod's lines ("<player> sent you <item>!", "You found <item> for <player>!") written into a free entry of 862's text bank (a/0/0/3 file 283) before script 2816 shows it; badge names (a/0/0/3 file 64, entries 11 to 18) |
| `mod_file_write`, `mod_file_read` | 6 | item 15 | the stored world (`world-<config hash>.json` solo, `online-<code>-<player>.json` online), the online checks (`checks-<code>-<player>.json`), and `join.txt` ("<code> <name>": a new game joins that world) |
| `world_join`, `world_checks`, `world_leave`, `world_read`, event `world` | 6 | item 16 | online worlds: join by name or key, checks as they happen and all again on each join, location 0 for the goal; the push (JOINED), items by index (ITEMS), errors (a `bad-key` joins again by name), positions |
| `peer_position` | 6 | item 17 | on after a join: the port shares the position and draws the others in the same zone |
| `townmap_marker_set`, `townmap_marker_clear` | 6 | item 17 | the other players' zones on the Town Map |
| `log`, `on`, `on_frame` | core | | log; bind the handlers; the frame tick (retries a full bag, starts the next message) |
| `game_code` | 6 | addition 1 | the cartridge's game code: version (Black or White) and language family |
| `rd32` | core | | only if `game_code` fails: the header copy at 0x027FFE0C or 0x02FFFE0C |
| `text_language` | 6 | item 2 | only if both fail: the language family (Black is then assumed) |
| `time_utc`, `ticks_ms` | 7 | | the seed of a new world: the clock, mixed (`rand_next` starts at a fixed seed in every run, so every world had the same one) |
| `item_add` | 5 | | gives items |
| `state_load`, `state_store` | 5 | | the per-save state: world file, slot, checked locations, items waiting for room, the online world or Archipelago room |
| `mode_get`, `mode_code` | 6 | | the way to play chosen on eidos's Multiplayer screen (`mode` lines of mod/settings.txt) and what its Join box gave: a world code, or an Archipelago room's line `ap <host:port> <slot> <password>` (read into 256 bytes; eidos makes it up to 160) |
| `setting_get`, `setting_text`, `player_name` | 6 | | the screen's options and the player's multiplayer name |
| `world_create`, `world_owner` | 6 | item 16 | a world made from the game: metaxy makes it (`N`), the owner starts it (`O`) |
| `dex_caught` | 6 | | the species caught, for the dex count checks |
| `ap_link`, `ap_send`, `ap_read`, event `ap` | 6 | item 16 | Archipelago rooms through metaxy's bridge: open the link to the room, send Archipelago messages (JSON arrays of commands), read one message (up to 2 MB) inside the `ap` handler (`crates/mod/src/ap.rs`) |
| `tracker_set` | 6 | | the field's Tracker (`crates/mod/src/tracker.rs`): one `area` line per place of `data/world/areas.json` (`tools/areas.py`: the place's zones from the cartridges' zone headers), a `check found/open/locked` line per location of the player (open: reachable with the items held, by the generator's own rules, `crates/gen/src/reach.rs`), `item` lines for the items that open locations (Badges, HMs, Key items, Seasons) with the count held, and the `count` line; made again when a check is found or an item comes, at most once a second, and sent only when it changed (a new game and a load send it again). Measured in the wasm module under node: the logic built once in about 25 ms, the text made in under 1 ms |

Events it binds: `game.new` (1: by the mode: make, store and apply a solo
world; join or make a multiworld; link to an Archipelago room; without a
mode, join the world of `join.txt`), `save.begin` (2: the state goes with
the game's save), `save.load` (4: load the save's world, apply it, join
again by key when online, link to the room again in Archipelago),
`game.cleared` (5: the ending; the goal is the first for "ghetsis", the
second for "champion"), `battle.end` (80) and `dex.register` (112: the
extra checks), `flag.set` (111: the check), `script.done` (114: either
message script ended), `starter.choices` (121: the world's three
starters), `world` (124: metaxy's answers; in Archipelago, the link's end),
`ap` (125: the Archipelago link's answers).

## Later

| Import or event | Proposal | What for |
|---|---|---|
| `text_input`, event `text_input.done` | item 14 | world code and player name in the game (now `join.txt`) |

## How a check works (the contract with the scripts)

The check edits (`crates/apply/src/scripts.rs`, applied at start) change
the shared give routines and the give sites so that, instead of giving
the original item, the script:

1. loads the original item and count into work 0x8008 and 0x8009 (0 for a
   badge);
2. loads the site's key into work 0x805E;
3. loads the marker 0xA4C7 into work 0x805F;
4. sets the location's flag (`SetEventFlag`; trainer gifts call
   `SetTrainerDefeatedFlag` again, flag 1420 + trainer);
5. clears work 0x805F and sets 0x8010 to 1 (the give succeeded).

The `flag.set` handler (bound before `ZONEDATA_SetEventFlag` 0x02014314,
so the giving script still runs and `work_get` sees its works): with the
marker, (flag, key) gives the location ids (`data/world/item_sites.json`;
the game flag where a site has `game_flag`). A location of the player's
world gives its item once (the save keeps the checked ids); a flag that is
no location of the world, or a location already checked (the game repeats
some gifts each day), gives the original item, or the badge. A flag set
without the marker is the game's own and is ignored.

Works 0x805E, 0x805F, 0x40FD, 0x40FE and 0x40FF are used by no original script in
the 12 cartridges and by no game code (0x4000-0x401F are the only saved
works the game clears).
