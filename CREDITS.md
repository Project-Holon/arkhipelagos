# Credits

- **PokemonBWAP** by BlastSlimey and contributors (MIT). The logic data in
  `data/world` was converted from its world data by `tools/bwap2data.py`
  and is maintained here from then on. Its field-script command table was
  a cross-check for ours. The first set of new in-game lines comes from
  its lines. Its species, move, learnset, encounter and trainer tables
  were the reference `tools/check_facts.py` checks our game facts against
  (`docs/facts.md`). The species names in `data/world/species_names.json`
  (names of the dexsanity and seensanity checks) come from its pokedex
  (`tools/bwap2names.py`), and the check options take its names and
  meaning (dexsanity, seensanity, dexcountsanity, trainersanity).
- **aletheia**, the decompilation of Pokemon Black and White: the
  field-script command table and every handler's arguments come from its
  source and layout data, and so do the record layouts `tools/facts.py`
  reads (personal data, learnsets, evolutions, moves, machines, type chart,
  encounters, trainers).
- **Archipelago** (MIT): the multiworld system arkhipelagos plays with.

ndspy (GPL 3) is not used.
