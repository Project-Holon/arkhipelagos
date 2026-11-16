# arkhipelagos

A randomizer and Archipelago mod for
[eidos](https://github.com/Project-Holon/eidos), the native PC port of
Pokemon Black and White. It works on all 12 cartridges (Black and White in
English, French, German, Italian, Spanish and Japanese) and needs no patch
file: players install the mod, then play alone, with other eidos players,
or in an Archipelago multiworld.

Two ways to play:

- **eidos randomizer**: alone (the game makes the world on your device) or
  with other eidos players (the world is made and kept on metaxy, eidos's
  relay; join any time, items are shared, you see each other on the map).
- **Archipelago**: like any other Archipelago game. The organiser
  generates with `arkhipelagos.apworld`; you type the room link and your
  slot name in the mod.

The mod ships no game data. It builds its changes from the player's own
cartridge files when the game starts.

## What is here

| Folder | What |
|---|---|
| `crates/gen` | the world generator (Rust); metaxy and the mod use the same code |
| `crates/script` | field-script disassembler and assembler, all 12 cartridges |
| `crates/formats` | NARC archives (and later text) |
| `crates/gen-wasm` | the generator built for wasm32 |
| `crates/apply` | turns a world's table changes into the player's own game files (no game data shipped), and the check edits of the give scripts |
| `crates/mod` | the eidos mod (wasm): makes a solo world, applies it, turns checks into items |
| `mod` | the mod folder: `eidos_mod.txt` (the wasm file is built into it) |
| `data/world` | the game's logic: items, regions, connections, events, locations, rules |
| `data/facts` | the original game's tables as plain numbers (stats, learnsets, encounters, trainers and more) |
| `data/script` | the field-script command table per language family |
| `tools` | Python tools that build and check the data from the cartridges |
| `tests` | test configurations, golden hashes, the check script |
| `docs` | how each part works, and the design proposals |
| `lines` | the mod's in-game lines and their translations |

Still to come here: the Archipelago world (`apworld`), and making online
worlds from inside the game (they are made on metaxy for now; a new game
joins the world named in the mod file `join.txt`). Releases will carry `mod.bw` and `mod.json` for katalogos.
The mod needs eidos's mod API version 6.

## Build and check

```
cargo build --release
sh tests/check.sh
```

The script check needs the 12 cartridges' extracted data; list where they
are in `tools/carts.local.json` (see `tools/carts.example.json`).
`sh tests/ci.sh` runs the checks that need no cartridge; GitHub Actions
runs it on every push. A tag `v*` builds the mod and publishes `mod.bw` and
`mod.json` with katalogos's release workflow.

## Documents

- `docs/script-tool.md`: the field-script tool and its checks
- `docs/generator.md`: the generator, its data and its checks
- `docs/facts.md`: the game facts, how they are read and checked
- `docs/eidos-api-proposal.md`: the eidos mod API it needs (approved)
- `docs/v6-calls.md`: the API calls the mod uses, and how a check works
- `docs/metaxy-proposal.md`: world sessions and the Archipelago bridge in
  metaxy (approved)

## Credits

- [PokemonBWAP](https://github.com/BlastSlimey/PokemonBWAP) by BlastSlimey
  and contributors (MIT): the logic data started from its world data, and
  its tables were the reference our game facts were checked against.
- [Archipelago](https://archipelago.gg) (MIT): the multiworld system.
- aletheia, the decompilation of Pokemon Black and White: the field-script
  commands and the layouts of every game table come from its source.
- [eidos](https://github.com/Project-Holon/eidos) and metaxy, which run
  the mod and its online play.

Details in [CREDITS.md](CREDITS.md).

## License

MIT (`LICENSE`).
