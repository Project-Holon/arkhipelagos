"""Convert PokemonBWAP's species logic (MIT) into data/world/species.json.

usage (with a Python that has Archipelago's dependencies):
  python tools/bwap2species.py <BWAP apworld dir (pokemon_bw)> <Archipelago dir>

What is converted (the logic only; the species themselves come from
data/facts and the randomizers):
- wild: each encounter area, season and method, the logic regions it is
  reached from, and its rule (surf, a rod, the Trio Badge for rippling
  spots, the season)
- statics: legendary, gift, fossil, static and starter encounters: the
  region, the inclusion and access rules, and the game's species per
  version (a check against the scripts, not used by the generator)
- trades: the region and the species wanted per version
- trainers: the region of each trainer, its inclusion and access rules, and
  whether it is a boss, a gym trainer or a rival

Rules are written in our rule language (see tools/bwap2data.py).
"""
import inspect
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
from bwap2data import load_package, rule_ref, include_ref, SOURCE  # noqa: E402

OUT = os.path.join(ROOT, "data", "world", "species.json")
METHOD = {"G": "grass", "DG": "double_grass", "RG": "shaking_grass", "S": "surf",
          "SR": "surf_ripple", "F": "fishing", "FR": "fishing_ripple"}
SEASON = {"Spring": 0, "Summer": 1, "Autumn": 2, "Winter": 3}


def main():
    load_package(sys.argv[1], sys.argv[2])
    import pokemon_bw.data as D  # noqa: F401
    from pokemon_bw.data.locations import rules as R
    from pokemon_bw.data.locations import story_events as EV
    from pokemon_bw.data.locations.encounters import regions as ER, region_connections as ERC, static as ST
    from pokemon_bw.data.trainers import data as TD

    names = {id(v): k for k, v in vars(R).items() if callable(v) and not isinstance(v, type)}
    chain_code = EV.chain_event("x").__code__
    todo = []

    wild = []
    for c in ERC.connections:
        area, season, m = c.entering_region
        rules = []
        if "S" in m:
            rules.append({"rule": "can_use_surf"})
        if "F" in m:
            rules.append({"rule": "can_fish"})
        if "R" in m:
            rules.append({"rule": "has_trio_badge"})
        if season:
            rules.append({"rule": "can_set_" + season.lower()})
        wild.append({
            "area": area,
            "file": ER.region_list[area].file,
            "season": SEASON[season] if season else None,
            "method": METHOD[m],
            "regions": list(c.exiting_regions),
            "include": {"include": "changeable_seasons"} if season else True,
            "rule": {"all": rules} if rules else True,
        })

    statics = []
    for kind, table in (("legendary", ST.legendary), ("gift", ST.gift), ("fossil", ST.fossils),
                        ("static", ST.static), ("starter", ST.starters)):
        for name, d in table.items():
            statics.append({
                "kind": kind, "name": name, "slot": d.patch_id,
                "black": list(d.species_black), "white": list(d.species_white),
                "region": d.encounter_region,
                "include": include_ref(d.inclusion_rule, names, todo),
                "rule": rule_ref(d.access_rule, names, chain_code, todo),
                "fixed": d.fixed,
            })
    trades = []
    for name, d in ST.trade.items():
        trades.append({"name": name, "black": list(d.species_black), "white": list(d.species_white),
                       "wanted_black": d.wanted_black, "wanted_white": d.wanted_white,
                       "region": d.encounter_region})
    trainers = []
    for t in TD.table:
        trainers.append({
            "id": t.id, "region": t.region,
            "include": include_ref(t.logic_inc_rule, names, todo),
            "rule": rule_ref(t.access_rule, names, chain_code, todo),
            "boss": t.is_boss, "rival": t.rival,
            "gym": list(t.gym) if t.gym else None,
        })
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        json.dump({"_source": SOURCE.replace("bwap2data", "bwap2species"), "wild": wild, "statics": statics,
                   "trades": trades, "trainers": trainers}, f, indent=1, ensure_ascii=False)
        f.write("\n")
    print(f"wild {len(wild)}, statics {len(statics)}, trades {len(trades)}, trainers {len(trainers)}")
    print(f"to do by hand: {len(todo)}")
    for t in sorted(set(todo))[:40]:
        print("   ", t)


if __name__ == "__main__":
    main()
