"""Read the original game's tables from the cartridges and write them as
plain facts (numbers keyed by game numbers) to data/facts/.

usage: python tools/facts.py [--check]

No game bytes are written: every value is a decoded number. Each table is
read from every cartridge of a version, and the facts must be the same in
all six languages; a table that differs between Black and White is written
once per version. --check reads everything and compares, but writes nothing.

Where each layout comes from (aletheia's source and its verified converters):
- personal a/0/1/6, 60-byte records, struct Personal (unit_0201AE14.c);
  TM/HM compatibility: four u32 at +0x28 (personal params 0x27 to 0x2A,
  sodateya.c), bit n = machine index n (FILE_UTIL_GetId: TM01-95 are
  0-94, HM01-06 are 95-100); member 668 is the form table (u16, 999 = none)
- learnsets a/0/1/8: (u16 move, u16 level) pairs, ended by 0xFFFF 0xFFFF
- evolutions a/0/1/9: 7 rows of (u16 method, u16 param, u16 species)
- moves a/0/2/1: 36-byte records, struct WazaData (aletheia waza.py since
  c69433ce23; the type is byte 0)
- machines: arm9 table g_0209EAA0 (English Black address), 101 u16 move
  numbers in the order of FILE_UTIL_GetMachineMove: TM01-92, HM01-06,
  TM93-95; found in each cartridge by the English table's content
- type chart: gBtlCalcTypeAffinityTable, u8[17][17] in ov093 (0x021F047C,
  English Black), codes 0 none, 2 half, 4 normal, 8 double
  (BTL_CALC_GetTypeAffinity)
- encounters a/1/2/6: 232-byte blocks (1 or 4, one per season), 7 rate
  bytes, a pad byte, 56 slots of (u16 species | form << 11, u8 min, u8 max)
- trainers a/0/9/2 (20-byte record: party type, class, ?, party count) and
  a/0/9/3 (party entries of 8, 16, 10 or 18 bytes by party type; tr_tool.c
  TR_TOOL_BuildParty)
"""
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import carts  # noqa: E402
from textcodec import narc_files  # noqa: E402

OUT = os.path.join(ROOT, "data", "facts")
SOURCE = "read from the cartridges by tools/facts.py; numbers only"

ARM9_BASE = 0x02004000
EN_MACHINES = 0x0209EAA0
OV093_BASE_EN = 0x021B6100
EN_TYPE_CHART = 0x021F047C
METHODS = ("grass", "double_grass", "shaking_grass", "surf", "surf_ripple", "fishing", "fishing_ripple")
SLOTS = (12, 12, 12, 5, 5, 5, 5)


def narc(cart, path):
    return narc_files(os.path.join(cart["data"], *path.split("/")))


def species_table(cart):
    out = {}
    files = narc(cart, "a/0/1/6")
    for i in range(1, 668):
        r = files[i]
        assert len(r) == 60, (i, len(r))
        hp, atk, de, spe, spa, spd, t0, t1, catch = struct.unpack_from("<9B", r, 0)
        items = list(struct.unpack_from("<3H", r, 0x0C))
        gender, hatch, friend, growth, eg1, eg2, ab1, ab2, ab3 = struct.unpack_from("<9B", r, 0x12)
        form_base, _form_comp, form_count = struct.unpack_from("<HHB", r, 0x1C)
        words = struct.unpack_from("<4I", r, 0x28)
        tm = [n for n in range(101) if words[n // 32] >> (n % 32) & 1]
        out[str(i)] = {
            "stats": [hp, atk, de, spe, spa, spd],
            "types": [t0, t1],
            "catch_rate": catch,
            "items": items,
            "gender": gender,
            "hatch": hatch,
            "friendship": friend,
            "growth": growth,
            "egg_groups": [eg1, eg2],
            "abilities": [ab1, ab2, ab3],
            "form_base": form_base,
            "forms": form_count,
            "machines": tm,
        }
    return out


def form_table(cart):
    d = narc(cart, "a/0/1/6")[668]
    return list(struct.unpack_from("<%dH" % (len(d) // 2), d, 0))


def learnsets(cart):
    out = {}
    for i, d in enumerate(narc(cart, "a/0/1/8")):
        if i == 0:
            continue
        pairs = [struct.unpack_from("<HH", d, k) for k in range(0, len(d), 4)]
        assert pairs[-1] == (0xFFFF, 0xFFFF), i
        out[str(i)] = [list(p) for p in pairs[:-1]]
    return out


def evolutions(cart):
    out = {}
    for i, d in enumerate(narc(cart, "a/0/1/9")):
        if i == 0:
            continue
        rows = [list(struct.unpack_from("<3H", d, 6 * k)) for k in range(7)]
        rows = [r for r in rows if r[2] != 0]
        if rows:
            out[str(i)] = rows
    return out


def moves(cart):
    out = {}
    for i, d in enumerate(narc(cart, "a/0/2/1")):
        if i == 0:
            continue
        assert len(d) == 36, i
        # struct WazaData (aletheia waza.py since c69433ce23): type, category
        # (the effect group), damageType (0 status, 1 physical, 2 special),
        # power, accuracy, basePP, priority; sickId (the ailment) at +8
        typ, group, damage, power, acc, pp, prio = struct.unpack_from("<6Bb", d, 0)
        ailment = struct.unpack_from("<H", d, 8)[0]
        out[str(i)] = {"type": typ, "group": group, "damage_type": damage, "power": power,
                       "accuracy": acc, "pp": pp, "priority": prio, "ailment": ailment}
    return out


def find_unique(blob, pattern, what, code):
    at = blob.find(pattern)
    if at < 0 or blob.find(pattern, at + 1) >= 0:
        raise SystemExit(f"{code}: {what} not found exactly once")
    return at


EN_BYTES = {}


def machines(cart, code):
    arm9 = open(cart["arm9"], "rb").read()
    if "machines" not in EN_BYTES:
        en = carts.load()["IRBO"]
        b = open(en["arm9"], "rb").read()
        EN_BYTES["machines"] = b[EN_MACHINES - ARM9_BASE:EN_MACHINES - ARM9_BASE + 202]
    at = find_unique(arm9, EN_BYTES["machines"], "machine table", code)
    mv = struct.unpack_from("<101H", arm9, at)
    out = []
    for n in range(101):
        if n < 92:
            name, item, slot = f"TM{n + 1:02d}", 0x148 + n, n
        elif n < 98:
            name, item, slot = f"HM{n - 91:02d}", 0x1A4 + n - 92, n - 92 + 95
        else:
            name, item, slot = f"TM{n - 5:02d}", 0x26A + n - 98, n - 98 + 92
        out.append({"name": name, "item": item, "move": mv[n], "compat_bit": slot})
    return out


def type_chart(cart, code):
    ov = open(os.path.join(cart["ov"], "y9_0093.bin"), "rb").read()
    if "chart" not in EN_BYTES:
        en = carts.load()["IRBO"]
        b = open(os.path.join(en["ov"], "y9_0093.bin"), "rb").read()
        off = EN_TYPE_CHART - OV093_BASE_EN
        EN_BYTES["chart"] = b[off:off + 289]
        assert set(EN_BYTES["chart"]) <= {0, 2, 4, 8}
    at = find_unique(ov, EN_BYTES["chart"], "type chart", code)
    return [list(ov[at + 17 * a:at + 17 * a + 17]) for a in range(17)]


def encounters(cart):
    out = {}
    for i, d in enumerate(narc(cart, "a/1/2/6")):
        assert len(d) in (232, 928), (i, len(d))
        seasons = []
        for b in range(len(d) // 232):
            blk = d[232 * b:232 * b + 232]
            rates = list(blk[:7])
            slots, k = {}, 0
            for m, n in zip(METHODS, SLOTS):
                rows = []
                for _ in range(n):
                    sf, lo, hi = struct.unpack_from("<HBB", blk, 8 + 4 * k)
                    rows.append([sf & 0x7FF, sf >> 11, lo, hi])
                    k += 1
                slots[m] = rows
            seasons.append({"rates": dict(zip(METHODS, rates)), "slots": slots})
        out[str(i)] = seasons
    return out


ENTRY = {0: 8, 1: 16, 2: 10, 3: 18}


def trainers(cart):
    data = narc(cart, "a/0/9/2")
    party = narc(cart, "a/0/9/3")
    out = {}
    for i in range(1, len(data)):
        r = data[i]
        assert len(r) == 20, (i, len(r))
        ptype, tclass, _b2, count = struct.unpack_from("<4B", r, 0)
        size = ENTRY[ptype]
        p = party[i]
        assert len(p) >= size * count, (i, len(p), size, count)
        mons = []
        for k in range(count):
            e = p[size * k:size * k + size]
            dv, ability, level, species, form = struct.unpack_from("<BBHHH", e, 0)
            m = {"species": species, "form": form, "level": level, "dv": dv, "ability": ability}
            o = 8
            if ptype in (2, 3):
                m["item"] = struct.unpack_from("<H", e, o)[0]
                o += 2
            if ptype in (1, 3):
                m["moves"] = list(struct.unpack_from("<4H", e, o))
            mons.append(m)
        out[str(i)] = {"class": tclass, "party": mons}
    return out


TABLES = {
    "species": lambda c, code: species_table(c),
    "forms": lambda c, code: form_table(c),
    "learnsets": lambda c, code: learnsets(c),
    "evolutions": lambda c, code: evolutions(c),
    "moves": lambda c, code: moves(c),
    "machines": machines,
    "type_chart": type_chart,
    "encounters": lambda c, code: encounters(c),
    "trainers": lambda c, code: trainers(c),
}


def main():
    check = "--check" in sys.argv
    C = carts.load()
    results = {}
    for name, fn in TABLES.items():
        per = {code: fn(C[code], code) for code in carts.CODES}
        by_version = {}
        for v in ("black", "white"):
            codes = [c for c in carts.CODES if carts.version(c) == v]
            first = per[codes[0]]
            bad = [c for c in codes[1:] if per[c] != first]
            if bad:
                raise SystemExit(f"{name}: {bad} differ from {codes[0]}")
            by_version[v] = first
        same = by_version["black"] == by_version["white"]
        results[name] = by_version["black"] if same else by_version
        print(f"{name}: same in all 6 languages of each version; "
              f"{'same in Black and White' if same else 'differs between Black and White'}")
    if check:
        return
    os.makedirs(OUT, exist_ok=True)
    for name, val in results.items():
        doc = {"_source": SOURCE}
        if isinstance(val, dict) and set(val) == {"black", "white"}:
            doc["black"], doc["white"] = val["black"], val["white"]
        else:
            doc["all"] = val
        with open(os.path.join(OUT, f"{name}.json"), "w", encoding="utf-8", newline="\n") as f:
            json.dump(doc, f, separators=(",", ":"), sort_keys=False)
            f.write("\n")
    print("wrote", OUT)


if __name__ == "__main__":
    main()
