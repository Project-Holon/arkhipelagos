# The world generator

`crates/gen` makes an arkhipelagos world from a game configuration. It is
one code for every place a world is made: metaxy links it for online games,
the eidos mod builds it in for solo games on the player's device. The logic
data is compiled in, so it reads no files.

## Data (`data/world`)

| File | What it holds |
|---|---|
| `items.json` | 621 items: id, name, group, class (progression, useful, filler, or a class that depends on options) |
| `regions.json` | 502 regions (maps and parts of maps), with an inclusion rule |
| `connections.json` | 572 connections, a rule each way, one-way flag, inclusion rule |
| `events.json` | 80 story events (fixed items the logic uses, such as "[Event] Defeating Ghetsis") |
| `locations.json` | 638 locations: item balls, hidden items, gifts, gym badges and TMs, each with its event flag |
| `rules.json` | 115 named rules and 10 inclusion rules, in the rule language |

The files were converted once from PokemonBWAP's world data (MIT) by
`tools/bwap2data.py`, and from now on they are ours, edited by hand. Each
file names its source. BWAP's rules were Python functions; the converter
reads their syntax tree and writes the same logic in our rule language (all
115 translated, three by hand: door shuffle, which is not in v1, a
Driftveil rule, and the Dreamyard monkeys inclusion).

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
{"has": NAME}, {"has": NAME, "count": N}
{"all": [...]}, {"any": [...]}, {"not": R}
{"reach": REGION}
{"rule": NAME}, {"include": NAME}
{"if": R, "then": R}
{"option": NAME, "eq": V}, {"option": NAME, "has": V}, {"option": NAME, "set": true}
{"value": NAME}
{"has_any_set": NAME}, {"has_all_set": NAME}, {"has_value": NAME}, {"seen_count": N}
{"has_all_group": "tm"}, {"has_any_group": NAME}
```

## Configuration and world

Configuration (made by the mod when a game is created):

```json
{"schema": 1, "seed": 4242,
 "options": {"goal": "ghetsis", "season_control": "vanilla"},
 "players": [{"name": "Ash", "version": "black"},
             {"name": "Misty", "version": "white", "options": {"modify_logic": ["require_flash"]}}]}
```

v1 options: `goal` (ghetsis, champion, tmhm_hunt), `season_control`
(vanilla, changeable), `modify_logic` (require_flash, require_dowsing),
`starter_pool` (species numbers to draw three starters from). A player's
options override the game's.

The world holds, per player: the effective options, every location of
their game with the item placed there and the player who gets it, the
locations v1 leaves out, and the starters. It also records the generator
version, the hash of the data files and the configuration's hash.

## How a world is made

1. Each player's game: regions, connections, locations and events that are
   on for their version and options.
2. Locations no item can ever unlock are left out (in v1: the 13 whose
   rule asks for species data; there the game keeps its own items).
3. Progression items of all players are placed by an assumed fill: in
   random order, each item goes to a random empty location that is
   reachable when every item not yet placed is assumed held. Locations of
   any player can hold any player's items.
4. The other locations get useful items, then filler.
5. The world must be finishable: a sweep from nothing collects every
   player's goal.

## v1 limits (until the vanilla game tables are decided)

- HMs: the HM is enough (BWAP also asks for a Pokemon that can learn it).
- Rules about species (seen counts, a species to show, type checks) are
  unmet, so those 13 locations are left out.
- No wild, trainer, stats or other randomizers that need the original
  tables; starters only from a list the configuration gives.
- Not yet: door shuffle, dexsanity and the other sanity checks, trainer
  checks.

## Checks (`tests/check.sh`)

- Determinism: the same configuration hashes the same in separate
  processes, and the native (x86-64) and wasm32 builds give the same hashes
  for all test configurations (`tests/golden.txt`, `tests/wasm_hash.mjs`).
- Every world can be finished: `arkh-gen sweep 1000`, random games of 1 to
  6 players with mixed versions, both goals, both season modes and logic
  options: 1000 of 1000 made and finishable.
- The logic holds items back: in the two-player test world, 50 of 1,208
  locations are reachable with nothing; the world opens over 15 spheres
  (`arkh-gen spheres`).
- Speed: 2 players 0.1 s, 16 players 1.6 s (release build).
