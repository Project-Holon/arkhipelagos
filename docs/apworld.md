# The Archipelago world (`apworld/arkhipelagos`)

`arkhipelagos.apworld` lets an Archipelago host generate a multiworld with
eidos players. It gives two games, one per cartridge version:
**Pokemon Black (arkhipelagos)** and **Pokemon White (arkhipelagos)**.
There is no patch file: the mod gets everything from slot_data, through
metaxy's Archipelago bridge.

## One logic, two languages, no copy

The world's Python reads only what the generator (`crates/gen`) writes:

- `data/static.json` (`arkh-gen ap-static`): every item and location with
  its permanent number (`docs/generator.md`), written into the apworld at
  build time.
- one **pack** per player (`arkh-gen ap-pack`): that player's game, made
  from their options and a seed: the tables the randomizers made, the
  regions, connections, locations and story events in their game, the
  species sources (wild, statics, gifts, trades, trainers), evolutions and
  form changes, every rule with the player's options folded in, the item
  pool (progression, useful, filler), and the data the mod needs.

Python turns a pack into Archipelago regions, entrances, locations and
items, and the rule language into access rules (`logic.py`). Species work
as in the generator's sweep: each source, evolution and form change is an
event; collecting it counts the species caught and seen, the species sets
hit and the levels reached. Python computes no names, numbers, pools,
tables or rules of its own.

`arkh-gen ap-reach` gives the locations one player reaches with a set of
items (the generator's own sweep). The world's tests compare it with the
Python rules for no items, every progression item and hundreds of random
sets, on Black, White and a game with every randomizer on.

## Which options need the generator on the host

A pack depends on the seed when the tables change, so the world runs the
generator (a small program, `arkh-gen`, inside the apworld for Windows,
Linux and macOS) for each such player. These options need it:

- any `randomize_*` option or `replace_evo_methods` that is not empty,
- a `starter_pool`,
- `species_sources` other than the default.
- `hidden_items` false, or any `dexsanity`, `seensanity`, `dexcountsanity` or `trainersanity`.

Every other game (the original tables, any goal, season control and
modify_logic) uses one of 32 packs made when the apworld is built, and
needs no program. Those packs were made with one fixed seed, so their
per-world picks (the species the Route 5 and Driftveil people ask for, the
Castelia studio's type, the Driftveil TM) are the same in every such game.

If the program cannot run (no program for that system, macOS blocks it, or
the cache folder does not allow programs), generation stops with a message
that says so and what to do. `ARKHIPELAGOS_GEN` can name an `arkh-gen` the
host built. CI checks that the program gives the same packs on Windows,
Linux and macOS (`tools/ap_hashes.py`).

## Options

The names and values are the generator's (`docs/generator.md`), so a
player file means the same in eidos-only games and in Archipelago.

| Option | Values | Default |
|---|---|---|
| `goal` | `ghetsis`, `champion` | `ghetsis` |
| `season_control` | `vanilla`, `changeable` | `vanilla` |
| `modify_logic` | list of `require_flash`, `require_dowsing` | `[]` |
| `species_sources` | list of `static`, `evolutions`, `form_change`, `trainers`, `trades` | `['evolutions', 'form_change', 'static', 'trainers']` |
| `starter_pool` | list of National Dex numbers, 1 to 649 | `[]` |
| `randomize_wild_pokemon` | list of `randomize`, `ensure_all`, `similar_stats`, `type_themed_areas` | `[]` |
| `randomize_static_pokemon` | `randomize`, `similar_stats`, `no_legendaries` | `[]` |
| `randomize_gift_pokemon` | `randomize`, `similar_stats`, `no_legendaries`, `split_monkeys` | `[]` |
| `randomize_legendary_pokemon` | `randomize`, `keep_legendary` | `[]` |
| `randomize_trade_pokemon` | `randomize_offer`, `randomize_request` | `[]` |
| `randomize_starter_pokemon` | `randomize`, `any_base`, `similar_stats`, `type_variety` | `[]` |
| `randomize_trainer_pokemon` | `randomize`, `similar_stats`, `type_themed`, `shuffle_gym_types`, `rivals_keep_starter` | `[]` |
| `randomize_evolutions` | `randomize`, `follow_type` | `[]` |
| `replace_evo_methods` | `trade` | `[]` |
| `randomize_tm_hm_compatibility` | `randomize`, `all_hms`, `all_tms`, `follow_evolutions` | `[]` |
| `randomize_types` | `shuffle`, `randomize`, `follow_evolutions` | `[]` |
| `randomize_base_stats` | `randomize`, `follow_evolutions`, `random_total` | `[]` |
| `randomize_catch_rates` | `shuffle`, `randomize` | `[]` |
| `randomize_gender_ratio` | `randomize` | `[]` |
| `randomize_abilities` | `randomize` | `[]` |
| `randomize_held_items` | `randomize` | `[]` |
| `randomize_levelup_movesets` | `randomize`, `match_types` | `[]` |
| `randomize_move_data` | `power`, `type`, `accuracy`, `pp` | `[]` |
| `randomize_type_chart` | `shuffle` | `[]` |
| `hidden_items` | `true`, `false` (false: hidden items keep the game's items and are no checks) | `true` |
| `dexsanity` | 0 to 649 species caught are checks | 0 |
| `seensanity` | 0 to 649 species seen are checks | 0 |
| `dexcountsanity` | `{Maximum: 0-649, Steps: 1-649, Leniency: 0-648}`: catching 1 to Maximum species are checks | `{Maximum: 0, Steps: 1, Leniency: 0}` |
| `trainersanity` | 0 to 600 won trainer battles are checks | 0 |
| `shuffle_badge_rewards` | `vanilla`, `shuffle`, `anything` | `anything` |
| `shuffle_tm_rewards` | `shuffle`, `hm_with_badge`, `anything` | `anything` |

Archipelago's common options (`progression_balancing`, `accessibility`,
`local_items` and the others) work as usual.

A player file:

```yaml
name: Ash
game: Pokemon White (arkhipelagos)
Pokemon White (arkhipelagos):
  goal: ghetsis
  season_control: vanilla
  modify_logic: ['require_flash']
  randomize_wild_pokemon: ['randomize', 'similar_stats']
```

## slot_data

The pack's `slot` part, plus:

| Key | What |
|---|---|
| `format` | `arkhipelagos-slot/1` |
| `generator`, `data_hash` | the generator's version and data hash |
| `player` | the Archipelago slot number |
| `config` | the one-player generator configuration that made the pack (with its seed) |
| `version` | `black` or `white` |
| `options` | the effective generator options |
| `tables`, `starters`, `picks` | as in an eidos-only world's player data |
| `left_out` | locations no item can unlock in this player's game (they keep the game's items) |
| `locations` | `[location, item, player]` for each of this player's locations: `item` is our item number when the item is this player's own, else 0; `player` is the receiving slot |

## The .apworld file

A zip named `arkhipelagos.apworld` holding one folder, `arkhipelagos/`,
with the world's Python, `data/static.json`, `data/packs/*.json`,
`bin/<system>/arkh-gen[.exe]`, the docs, and `archipelago.json`:

```json
{"game": "Pokemon Black (arkhipelagos)", "minimum_ap_version": "0.6.8",
 "world_version": "0.1.0", "authors": ["Project-Holon"],
 "version": 7, "compatible_version": 7}
```

`version` and `compatible_version` are Archipelago's container version
(`worlds/Files.py`), which its "Build APWorlds" tool adds. The manifest
names one game; the second game registers from the same code. Archipelago
shows `world_version` only for the game the manifest names, and its general
test `test_world_manifest` fails for the second game for the same reason
(it expects one game per world folder); every other general test passes.

`python tools/build_apworld.py --gen <arkh-gen> --helper <system>=<arkh-gen> --out <dir>`
builds it; the release workflow builds the three programs, checks they give
the same packs, and attaches the file to the release.

Install: put the file in Archipelago's `custom_worlds` folder (or open it
with the Archipelago Launcher). The world's tests (`test/`) run from an
Archipelago source checkout with the world folder in `worlds/`, its
`data/static.json` written by `arkh-gen ap-static`, and
`ARKHIPELAGOS_GEN` set:
`python -m pytest worlds/arkhipelagos/test`.

## Sources

The world follows Archipelago's world API and apworld format
(Archipelago, MIT, `docs/world api.md`, `docs/apworld specification.md`,
and its APQuest example world). The game logic data came from PokemonBWAP
(MIT) once, as `docs/generator.md` says.
