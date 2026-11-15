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
| `data/world` | the game's logic: items, regions, connections, events, locations, rules |
| `data/script` | the field-script command table per language family |
| `tools` | Python tools that build and check the data from the cartridges |
| `tests` | test configurations, golden hashes, the check script |
| `docs` | how each part works, and the design proposals |
| `lines` | the mod's in-game lines and their translations |

Still to come here: the Archipelago world (`apworld`) and the eidos mod
(`mod`). Releases will carry `mod.bw` and `mod.json` for katalogos.

## Build and check

```
cargo build --release
sh tests/check.sh
```

The script check needs the 12 cartridges' extracted data; list where they
are in `tools/carts.local.json` (see `tools/carts.example.json`).

## Documents

- `docs/script-tool.md`: the field-script tool and its checks
- `docs/generator.md`: the generator, its data and its checks
- `docs/eidos-api-proposal.md`: the eidos mod API it needs (proposal)
- `docs/metaxy-proposal.md`: world sessions and the Archipelago bridge in
  metaxy (proposal)

## License

MIT (`LICENSE`). Credits in `CREDITS.md`.
