# Game facts (`data/facts`)

The original game's tables, as plain numbers keyed by game numbers. No
game bytes are stored: `tools/facts.py` reads the cartridges, decodes each
record and writes the values. The generator compiles these files in.

| File | What it holds | Key |
|---|---|---|
| `species.json` | base stats (HP, Attack, Defense, Speed, Sp. Atk, Sp. Def), types, catch rate, wild held items, gender ratio, hatch cycles, friendship, growth rate, egg groups, abilities, form record base and count, TM/HM compatibility (machine indices) | personal record number (species, then extra forms) |
| `forms.json` | the form table (personal member 668) | |
| `learnsets.json` | level-up moves, `[move, level]` | personal record number |
| `evolutions.json` | `[method, parameter, target]` | personal record number |
| `moves.json` | type, group (the game's "category", the effect group), damage type (0 status, 1 physical, 2 special), power, accuracy, PP, priority, ailment | move number |
| `machines.json` | TM01-95 and HM01-06: item number, move, compatibility bit | |
| `type_chart.json` | 17 x 17, the game's codes (0, 2, 4, 8) | attacking type, defending type |
| `encounters.json` | per encounter file, per season block: method rates and slots `[species, form, min, max]` | encounter file number; `black` and `white` |
| `trainers.json` | class and party (species, form, level, dv, ability; item and moves where the party has them) | trainer number |

## How they are checked

- **Same in every language.** Each table is read from all six cartridges
  of a version; the run stops if one differs. Only the encounters differ
  between Black and White, so that file has a `black` and a `white` part.
- **Layouts from the game's code.** Each layout is named in `facts.py`
  with the aletheia function that reads it.
- **Against BWAP** (`tools/check_facts.py`, needs BWAP and Archipelago):
  moves, machines, base stats, types, catch rates, gender ratios, growth
  rates, TM/HM compatibility, learnsets, 4,012 encounter slots of both
  versions, and every trainer's party (species, level, dv) agree. The
  differences that remain are all explained below.

## Differences from BWAP, explained

- **Evolutions (32 species).** BWAP's table is not the original: it
  already holds its own replacements. Trade evolutions (Kadabra, Machoke,
  Graveler, Haunter, Boldore, Gurdurr) are level-ups there; trade with an
  item (Onix, Scyther, Porygon and others) is "level up holding the item"
  day and night; Karrablast and Shelmet evolve with each other in the
  party; friendship day and night (Budew, Riolu, Chingling) are plain
  friendship; Feebas's beauty method is gone. Our file keeps the game's
  methods; the replacements are a randomizer option.
- **Methods 23 and 24.** In the game, 24 is "level up, female" (Burmy
  into Wormadam, Combee into Vespiquen) and 23 is "level up, male" (Burmy
  into Mothim). BWAP's method table names them the other way round, so
  its data and ours agree on the numbers but not on the names.
- **Learnsets of six form records** (Deoxys Attack, Defense and Speed,
  Wormadam Sandy and Trash, Shaymin Sky). BWAP gives these the base form's
  learnset; the cartridge has their own (Deoxys Speed learns Double Team at
  17, not Teleport).
- **One encounter slot**: White, file 95, spring double grass slot 4 is
  level 36 to 36 on the cartridge; BWAP writes 31 to 31.

## A note on the move record

The fields follow the game's `struct WazaData`, as aletheia's
`tools/assets/waza.py` names them since c69433ce23: byte 0 type, byte 1
category (the effect group), byte 2 damage type, then power, accuracy,
base PP, priority, and the ailment (`sickId`) at +8. Our names:
`type`, `group`, `damage_type`, `power`, `accuracy`, `pp`, `priority`,
`ailment`. Before that fix waza.py's map was one byte off; this file was
read from the data (Karate Chop reads 1, Fighting, in byte 0; Fire Punch
9, Fire) and every move in BWAP's table matches.
