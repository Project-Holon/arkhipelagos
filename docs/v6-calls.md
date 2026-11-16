# The eidos API calls the arkhipelagos mod uses

The mod is a wasm code mod (`crates/mod`, built into `mod/arkhipelagos.wasm`
by `tools/build_mod.sh`; manifest `mod/eidos_mod.txt`, `api = 6`). Item
numbers point to `docs/eidos-api-proposal.md` (approved, interface
version 6). Names and signatures are those of eidos's generated header
`sdk/api/v6/eidos_wasm.h`; `tools/wasm_imports.py` checks the built
module against it (`EIDOS_SDK=<eidos>/sdk sh tests/check.sh`).

## What the module imports now (solo flow)

19 imports, all from module `eidos`:

| Import | Version | Proposal | What for |
|---|---|---|---|
| `vfs_member_get` | 6 | item 1 | the player's own members, to build on (called with size 0 first: it returns the whole size) |
| `vfs_member_set` | 6 | item 1 | the changed members: scripts (a/0/5/7), personal, learnsets, evolutions, moves, encounters, trainer parties |
| `vfs_member_clear` | 6 | item 1 | `("", 0xFFFFFFFF)`, all of the mod's members, before each apply and for a save with no world |
| `type_chart_set` | 6 | item 11 | the world's type chart, 289 bytes, `cell[atk * 17 + def]` |
| `work_get` | 6 | item 3 | inside `flag.set`: works 0x805F (marker), 0x805E (key), 0x8008 and 0x8009 (original item and count) |
| `work_set` | 6 | item 3 | saved works 0x40FE and 0x40FF, the item and count the message script shows |
| `badge_set` | 6 | item 4 | a badge received as an item, or a badge location that is not in the world |
| `script_run` | 6 | item 5 | the message script 2815 ("You obtained X!") after an item |
| `mod_file_write`, `mod_file_read` | 6 | item 15 | the stored solo world (`world-<config hash>.json`) |
| `log`, `on`, `on_frame` | core | | log; bind the handlers; the frame tick (retries a full bag, starts the next message) |
| `rd32` | core | | the cartridge's game code from the header copy at 0x027FFE0C or 0x02FFFE0C, if the port fills it |
| `text_language` | 6 | item 2 | the language family when there is no header copy (Black is then assumed; a `game_code` call is requested in the proposal) |
| `rand_next` | 5 | | the seed of a new solo world |
| `item_add` | 5 | | gives items |
| `state_load`, `state_store` | 5 | | the per-save state: world file, slot, checked locations, items waiting for room |

Events it binds: `game.new` (1: make, store and apply a solo world),
`save.load` (4: load the save's world and apply it), `flag.set` (111: the
check), `script.done` (114: the message ended), `starter.choices` (121:
the world's three starters).

## Later (online and Archipelago)

| Import or event | Proposal | What for |
|---|---|---|
| `text_get`, `text_set`, `text_language` | item 2 | the mod's own lines ("found X for Y", "Y sent you X"), per-world pick lines |
| `text_input`, event `text_input.done` | item 14 | world code and player name; the title is UTF-8 |
| `world_create`, `world_owner`, `world_join`, `world_checks`, `world_leave` | item 16 | metaxy world sessions |
| event `world`, `world_read` | item 16 | the world's answers come as an event; the mod reads the bytes with `world_read` (player data is too big for a handler copy) |
| `ap_link`, `ap_send`, event `ap`, `ap_read` | item 16 | Archipelago rooms, the same way |
| `peer_position`, `townmap_marker_set`, `townmap_marker_clear` | item 17 | other players of the world |

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
no location of the world gives the original item, or the badge. A flag set
without the marker is the game's own and is ignored.

Works 0x805E, 0x805F, 0x40FE and 0x40FF are used by no original script in
the 12 cartridges and by no game code (0x4000-0x401F are the only saved
works the game clears).
