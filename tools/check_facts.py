"""Compare data/facts with PokemonBWAP's tables (the reference).

usage (with a Python that has Archipelago's dependencies):
  python tools/check_facts.py <BWAP apworld dir (pokemon_bw)> <Archipelago dir>

Prints every difference. A difference is either a mistake in our reading or
in BWAP's table; each one is looked at and explained in docs/facts.md.
"""
import json
import os
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
from bwap2data import load_package  # noqa: E402


def facts(name, version=None):
    d = json.load(open(os.path.join(ROOT, "data", "facts", f"{name}.json"), encoding="utf-8"))
    if "all" in d:
        return d["all"]
    return d[version]


def main():
    load_package(sys.argv[1], sys.argv[2])
    import pokemon_bw.data as D  # noqa: F401
    from pokemon_bw.data.pokemon import species as SP, types as TY, moves as MV, movesets_tm_hm as TMH
    from pokemon_bw.data.pokemon import movesets_level_up as LUP, evolution_methods as EVM
    from pokemon_bw.data.locations.encounters import slots as SL
    from pokemon_bw.data.trainers import pokemon as TP, data as TD

    sp = facts("species")
    lu = facts("learnsets")
    ev = facts("evolutions")
    mv = facts("moves")
    mach = facts("machines")
    diffs = Counter()
    shown = Counter()

    def diff(kind, msg):
        diffs[kind] += 1
        if shown[kind] < int(os.environ.get("SHOW", "8")):
            print(f"[{kind}] {msg}")
            shown[kind] += 1

    def index_of(dex, form):
        base = sp[str(dex)]["form_base"]
        if form == 0 or base == 0:
            # a form with no record of its own shares the species' record
            return dex
        return base + form - 1

    move_id = {n: d.id for n, d in MV.by_name.items()}
    method_id = {n: d.id for n, d in EVM.methods.items()}
    # moves
    for name, d in MV.by_name.items():
        f = mv.get(str(d.id))
        if f is None:
            diff("moves", f"{name}: no record {d.id}")
            continue
        cat = {"Physical": 0, "Special": 1, "Status": 2}
        got = (f["type"], f["power"], f["accuracy"], f["pp"])
        want = (TY.by_name[d.type], d.power, d.accuracy, d.pp)
        if got != want:
            diff("moves", f"{name} ({d.id}): ours {got}, BWAP {want}")
        if f["category"] != {"Physical": 1, "Special": 2, "Status": 0}[d.category]:
            diff("move-category", f"{name}: ours {f['category']}, BWAP {d.category} ({cat[d.category]})")
    # machines
    for m in mach:
        t = MV.tm_hm.get(m["name"])
        if t is None or move_id[t.move] != m["move"]:
            diff("machines", f"{m['name']}: ours move {m['move']}, BWAP {t}")
    # species
    for key, s in SP.by_name.items():
        i = index_of(s.dex_number, s.form)
        f = sp.get(str(i))
        if f is None:
            diff("species", f"{key}: no record {i}")
            continue
        st = f["stats"]
        ours = (st[0], st[1], st[2], st[4], st[5], st[3])
        if ours != tuple(s.base_stats):
            diff("stats", f"{key} ({i}): ours {ours}, BWAP {s.base_stats}")
        if tuple(f["types"]) != tuple(TY.by_name[t] for t in s.types):
            diff("types", f"{key} ({i}): ours {f['types']}, BWAP {s.types}")
        if f["catch_rate"] != s.catch_rate:
            diff("catch", f"{key}: ours {f['catch_rate']}, BWAP {s.catch_rate}")
        if f["gender"] != s.gender_ratio:
            diff("gender", f"{key}: ours {f['gender']}, BWAP {s.gender_ratio}")
        if f["growth"] != s.exp_curve:
            diff("growth", f"{key}: ours {f['growth']}, BWAP {s.exp_curve}")
        tm = TMH.table.get(key)
        if tm is not None:
            bits = {m["compat_bit"]: m["name"] for m in mach}
            ours_tm = {bits[b] for b in f["machines"]}
            if ours_tm != set(tm.tm_hm_moves):
                diff("machines-compat", f"{key} ({i}): only ours {sorted(ours_tm - set(tm.tm_hm_moves))}, "
                                         f"only BWAP {sorted(set(tm.tm_hm_moves) - ours_tm)}")
        lv = LUP.table.get(key)
        if lv is not None:
            want = [[move_id[m], l] for l, m in lv.level_up_moves]
            if want != lu.get(str(i)):
                diff("learnsets", f"{key} ({i}): ours {lu.get(str(i))[:4]}..., BWAP {want[:4]}...")
        evs = [(method_id[m], v if isinstance(v, int) else 0, t) for m, v, t in s.evolutions]
        ours_ev = ev.get(str(i), [])
        if [e[0] for e in evs] != [e[0] for e in ours_ev]:
            diff("evolutions", f"{key} ({i}): ours {ours_ev}, BWAP {s.evolutions}")
    # encounters
    enc = {v: facts("encounters", v) for v in ("black", "white")}
    from pokemon_bw.data.locations.encounters.regions import region_list  # noqa: F401
    methods = ["grass", "double_grass", "shaking_grass", "surf", "surf_ripple", "fishing", "fishing_ripple"]
    sizes = [12, 12, 12, 5, 5, 5, 5]
    seen = 0
    for e in SL.table:
        f, season, slot = e.file_index
        k = slot
        for m, n in zip(methods, sizes):
            if k < n:
                break
            k -= n
        for v, want in (("black", e.species_black), ("white", e.species_white)):
            blocks = enc[v][str(f)]
            blk = blocks[season] if season < len(blocks) else None
            if blk is None:
                diff("encounters", f"file {f} season {season}: we have {len(blocks)} blocks")
                continue
            got = blk["slots"][m][k]
            if (got[0], got[1]) != tuple(want) or (got[2], got[3]) != (e.min_level, e.max_level):
                diff("encounters", f"{v} file {f} season {season} slot {slot} ({m} {k}): ours {got}, "
                                   f"BWAP {want} {e.min_level}-{e.max_level}")
        seen += 1
    print(f"encounter slots compared: {seen}")
    # trainers
    tr = facts("trainers")
    species_by_name = {n: index_of(s.dex_number, s.form) for n, s in SP.by_name.items()}
    dex_by_name = {n: s.dex_number for n, s in SP.by_name.items()}
    for t in TD.table:
        f = tr.get(str(t.id))
        if f is None or len(f["party"]) != t.pokemon_count:
            diff("trainer-count", f"trainer {t.id} {t.name}: ours {None if f is None else len(f['party'])}, "
                                  f"BWAP {t.pokemon_count}")
    for p in TP.table:
        f = tr.get(str(p.trainer_id))
        if f is None or p.team_number >= len(f["party"]):
            diff("trainer-mon", f"trainer {p.trainer_id} #{p.team_number}: missing")
            continue
        m = f["party"][p.team_number]
        want = (dex_by_name.get(p.species, species_by_name.get(p.species)), p.level, p.ivs)
        got = (m["species"], m["level"], m["dv"])
        if got != want:
            diff("trainer-mon", f"trainer {p.trainer_id} #{p.team_number}: ours {got}, BWAP {want} ({p.species})")
    print()
    for k, n in sorted(diffs.items()):
        print(f"{k}: {n} differences")
    if not diffs:
        print("no differences")


if __name__ == "__main__":
    main()
