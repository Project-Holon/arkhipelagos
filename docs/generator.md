# The world generator

`crates/gen` makes an arkhipelagos world from a game configuration. It is
one code for every place a world is made: metaxy links it for online games,
the eidos mod builds it in for solo games on the player's device. The logic
data and the game facts are compiled in, so it reads no files.

## Data (`data/world`)

| File | What it holds |
|---|---|
| `items.json` | 621 items: id, name, group, class (progression, useful, filler, or a class that depends on options) |
| `regions.json` | 502 regions (maps and parts of maps), with an inclusion rule |
| `connections.json` | 572 connections, a rule each way, one-way flag, inclusion rule |
| `events.json` | 80 story events (fixed items the logic uses, such as "[Event] Defeating Ghetsis") |
| `locations.json` | 638 locations: item balls, hidden items, gifts, gym badges and TMs, each with its event flag |
| `rules.json` | 115 named rules and 10 inclusion rules, in the rule language |
| `species.json` | where species are found: 444 wild sources (area, season, method, the regions it is reached from, its rule), 40 statics (legendary, gift, fossil, static, starter), 5 trades, and the region of each of the 615 trainers |
| `species_rules.json` | evolution methods (by the game's method number) and where evolution items can be had, the species sets rules ask for, and form changes (Deerling and Sawsbuck by season, Rotom) |

The game facts (`data/facts`, see `docs/facts.md`) give what is found at
each place: wild slots per version and season, trainer parties, species
types and TM/HM compatibility, evolutions and learnsets.

`items` to `rules` were converted once from PokemonBWAP's world data (MIT)
by `tools/bwap2data.py`, `species.json` by `tools/bwap2species.py`; from
then on they are ours, edited by hand. BWAP's rules were Python functions;
the converters read their syntax tree and write the same logic in our rule
language. `tools/fix_species_rules.py` turned the rules that name a species
as an item ("Celebi") into species tests.

Permanent numbers (Archipelago ids, never changed once released):
- items: the game's item number; badges 0x1000 + badge bit; seasons
  0x1100 + index;
- locations: 0x10000 + (event flag << 4) + n, where n is the number of the
  gift when one person gives several ("#2"), and 8 for the TM a gym leader
  gives (it shares the badge's flag).

They come from game facts, never from list order.

## The rule language

```
true, false
{"has": NAME}, {"has": NAME, "count": N}, {"has_id": ITEM_NUMBER}
{"all": [...]}, {"any": [...]}, {"not": R}
{"reach": REGION}
{"rule": NAME}, {"include": NAME}
{"if": R, "then": R}
{"option": NAME, "eq": V}, {"option": NAME, "has": V}, {"option": NAME, "set": true}
{"value": NAME}
{"has_any_set": NAME}      caught a species of the set (species_rules.json "sets")
{"species": [DEX, FORM]}   caught that species and form
{"seen_count": N}          N species seen or caught
{"level": N}               a Pokemon can be raised to level N
{"has_value": NAME}        the per-world picks
{"has_all_group": "tm"}, {"has_any_group": NAME}
```

## Species in the logic

The sweep keeps, per player, which species and forms are caught, which
species are seen, and the highest level reached:

- **Wild**: a source is collected when one of its regions is reached and
  its rule holds (surf, a rod, the Trio Badge for rippling spots, a season
  that can be set). Seasonal sources count only when seasons can change.
- **Statics, trades, form changes, evolutions**: the same, with their own
  rules. An evolution needs the base species and its method: a level, an
  item that can be had (a shop or a place), the friendship checker, a
  special place, a known move. Trade evolutions are not in the logic.
- **Trainers**: their species are seen (for the seen counts), and their
  levels count like wild levels.
- **Levels** count in steps of 5, as BWAP's: a level N evolution needs a
  reached place whose level is at least N rounded down to 5. The Hall of
  Fame counts as level 100.
- **HMs** need the HM and a caught species that can learn it (Flash: TM70).
- **Picks**: one catchable species for the Route 5 and Driftveil people,
  one of its types for the Castelia studio, one of its TMs for the
  Driftveil lady (that TM becomes progression for the player). They are in
  the world (`picks`) for the mod to put into the scripts.

Option `species_sources` (default `static`, `evolutions`, `form_change`,
`trainers`; `trades` can be added) chooses what counts besides wild
Pokemon.

## Configuration and world

Configuration (made by the mod when a game is created):

```json
{"schema": 1, "seed": 4242,
 "options": {"goal": "ghetsis", "season_control": "vanilla"},
 "players": [{"name": "Ash", "version": "black"},
             {"name": "Misty", "version": "white", "options": {"modify_logic": ["require_flash"]}}]}
```

Options: `goal` (ghetsis, champion), `season_control` (vanilla,
changeable), `modify_logic` (require_flash, require_dowsing),
`species_sources`, `starter_pool` (species numbers to draw three starters
from). A player's options override the game's.

The world holds, per player: the effective options, every location of
their game with the item placed there and the player who gets it, the
locations no item can unlock (none with the default options), the
starters and the picks. It also records the generator version, the hash of
the data files and the configuration's hash.

## How a world is made

1. Each player's game: regions, connections, locations, events and
   species sources that are on for their version and options.
2. Locations no item can ever unlock are left out.
3. Progression items of all players are placed by an assumed fill: in
   random order, each item goes to a random empty location that is
   reachable when every item not yet placed is assumed held. Locations of
   any player can hold any player's items.
4. The other locations get useful items, then filler.
5. The world must be finishable: a sweep from nothing collects every
   player's goal.

Random numbers: the fill uses the configuration's stream; each other stage
of each player has its own stream (`gen::stream(cfg, stage, player)`), so a
new option or stage does not reshuffle the others.

## Checks (`tests/check.sh`, `tests/ci.sh`)

- Determinism: the same configuration hashes the same in separate
  processes, and the native (x86-64) and wasm32 builds give the same hashes
  for all test configurations (`tests/golden.txt`, `tests/wasm_hash.mjs`).
  The data hash skips carriage returns, so Windows and Linux checkouts
  agree.
- Every world can be finished: `arkh-gen sweep 300`, random games of 1 to
  6 players with mixed versions, both goals, both season modes and logic
  options: 300 of 300 made and finishable.
- Nothing is left out: with the default options every location of every
  test world is in the world (1,234 in the two-player test).
- The logic holds items back: in the two-player test world, 50 of 1,234
  locations are reachable with nothing; the world opens over 13 spheres
  (`arkh-gen spheres`).
- Speed (release build, x86-64): 1 player 0.2 s, 2 players 0.15 s,
  8 players 0.9 s, 16 players 3.2 s. The wasm module is 2.9 MB with the
  logic and facts in it.
