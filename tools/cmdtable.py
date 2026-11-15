"""Read the field-script command table from each cartridge's ov010.

usage: python tools/cmdtable.py [out.json]

English Black: the VM is created with params.cmds = d_0217062C and
params.cmdCount = d_021705D8 (aletheia src/game/ov010/script_sys.c). The
table is an array of Thumb function pointers. Other cartridges: the same
two objects, found through aletheia's language maps (layout/langmap) for
the European ones; for Japanese, by matching the shape of the table (the
gaps between neighbouring function addresses) against English.

Output: per cartridge the count, the table address, and per opcode the
function address and, where known, aletheia's English function name.
"""
import bisect
import csv
import difflib
import json
import os
import struct
import sys

import carts

C = carts.load()
AL = C["_aletheia"]
EN_TABLE, EN_COUNT = 0x0217062C, 0x021705D8


def y9(root):
    t = open(os.path.join(root, "y9.bin"), "rb").read()
    return {struct.unpack_from("<I", t, 32 * i)[0]: struct.unpack_from("<I", t, 32 * i + 4)[0] for i in range(len(t) // 32)}


class Image:
    """arm9 + ov010 + ov021 of one cartridge, addressed by RAM address."""

    def __init__(self, code):
        c = C[code]
        hdr = open(os.path.join(c["root"], "header.bin"), "rb").read()
        self.segs = [(struct.unpack_from("<I", hdr, 0x28)[0], open(c["arm9"], "rb").read())]
        bases = y9(c["root"])
        for ov in (10, 21):
            data = open(os.path.join(c["ov"], f"y9_{ov:04d}.bin"), "rb").read()
            self.segs.append((bases[ov], data))
        self.ov010 = (bases[10], len(self.segs[1][1]))

    def u32(self, addr):
        for base, data in self.segs:
            if base <= addr < base + len(data) - 3:
                return struct.unpack_from("<I", data, addr - base)[0]
        raise KeyError(hex(addr))

    def code_addr(self, a):
        a &= ~1
        return any(base <= a < base + len(d) for base, d in self.segs)


def langmap(code):
    names = {"IRAO": "white", "IRBF": "black_fr", "IRAF": "white_fr", "IRBD": "black_de", "IRAD": "white_de",
             "IRBI": "black_it", "IRAI": "white_it", "IRBS": "black_es", "IRAS": "white_es"}
    if code not in names:
        return None
    rows = []
    with open(os.path.join(AL, "layout", "langmap", names[code] + ".tsv"), encoding="utf-8") as f:
        for r in csv.DictReader(f, delimiter="\t"):
            rows.append((int(r["english_start"], 16), int(r["english_end"], 16), int(r["target_start"], 16)))
    rows.sort()
    return rows


def en_to_target(rows, a):
    starts = [r[0] for r in rows]
    i = bisect.bisect_right(starts, a) - 1
    if i >= 0 and rows[i][0] <= a < rows[i][1]:
        return rows[i][2] + (a - rows[i][0])
    return None


def target_to_en(rows, a):
    for s, e, t in rows:
        if t <= a < t + (e - s):
            return s + (a - t)
    return None


def en_names():
    """address -> list of (name, file): overlays share RAM addresses, so one
    address can hold several functions (in different overlays)."""
    out = {}
    with open(os.path.join(AL, "layout", "function_addrs.tsv"), encoding="utf-8") as f:
        for r in csv.DictReader(f, delimiter="\t"):
            out.setdefault(int(r["address"], 16), []).append((r["name"], r["file"]))
    return out


def read_table(img, table, count):
    return [img.u32(table + 4 * i) for i in range(count)]


def main():
    out_path = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(carts.HERE), "out", "script", "cmdtable.json")
    names = en_names()
    en = Image("IRBO")
    en_count = en.u32(EN_COUNT)
    en_ptrs = read_table(en, EN_TABLE, en_count)
    res = {}
    for code in carts.CODES:
        img = en if code == "IRBO" else Image(code)
        rows = langmap(code)
        if code == "IRBO":
            table, cnt, ptrs, en_of = EN_TABLE, en_count, en_ptrs, list(en_ptrs)
        elif rows is not None:
            table, cnt_addr = en_to_target(rows, EN_TABLE), en_to_target(rows, EN_COUNT)
            cnt = img.u32(cnt_addr)
            ptrs = read_table(img, table, cnt)
            en_of = [target_to_en(rows, p & ~1) for p in ptrs]
            en_of = [(a | 1) if a is not None else None for a in en_of]
        else:
            table, cnt, ptrs, _ = find_by_shape(img, en_ptrs)
            fm = jp_fnmap(code)
            en_of = [fm.get(p & ~1) for p in ptrs]
            en_of = [(a | 1) if a is not None else None for a in en_of]
            # aletheia's maps name most entries; the rest follow the measured
            # rule (JA_TO_EN). Check the rule against every mapped entry.
            agree = bad = 0
            for op, e in enumerate(en_of):
                rule = ja_to_en(op)
                if e is not None and ptrs[op]:
                    if rule is not None and en_ptrs[rule] == e:
                        agree += 1
                    else:
                        bad += 1
            print(f"   {code}: the opcode rule agrees with aletheia's map on {agree} entries, disagrees on {bad}"
                  " (the disagreements are map collisions: one Japanese address, several English overlays)")
            how = []
            for op in range(len(en_of)):
                rule = ja_to_en(op)
                if rule is not None and ptrs[op]:
                    en_of[op] = en_ptrs[rule]
                    how.append("rule")
                else:
                    en_of[op] = None
                    how.append("japanese-only" if ptrs[op] else None)
        entries = []
        for op, p in enumerate(ptrs):
            e = en_of[op]
            nm = names.get((e or 0) & ~1, [])
            entries.append({"op": op, "addr": f"{p:#010x}", "en_addr": f"{e:#010x}" if e else None,
                            "name": nm[0][0] if nm else None, "file": nm[0][1] if nm else None,
                            "candidates": [n for n, _ in nm]})
            if code[3] == "J":
                entries[-1]["en_op"] = ja_to_en(op)
                entries[-1]["named_by"] = how[op]
        named = sum(1 for x in entries if x["name"])
        en_names_by_op = [names.get(p & ~1, [(None, None)])[0][0] for p in en_ptrs]
        if code[3] != "J":
            agree = sum(1 for x in entries if x["name"] and x["name"] == en_names_by_op[x["op"]])
            print(f"   {code}: {agree} of {named} named entries carry the English name of the same opcode")
        res[code] = {"table": f"{table:#010x}", "count": cnt, "entries": entries}
        print(f"{code}: table {table:#010x}, {cnt} commands, {named} named")
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    json.dump(res, open(out_path, "w"), indent=1)


def ja_to_en(op):
    """Japanese opcode -> English opcode, or None for the three Japanese-only
    word-buffer commands (0x4D, 0x55, 0x5E). Measured from aletheia's
    Japanese function maps; English 0x4E, 0x4F, 0x57/0x58 (one of them) and
    0x61/0x62 (one of them) have no Japanese entry."""
    if op < 0x4D:
        return op
    if op in (0x4D, 0x55, 0x5E):
        return None
    if op <= 0x54:
        return op + 2
    if op <= 0x5D:
        return op + 3
    return op + 4


def jp_fnmap(code):
    """aletheia's Japanese builds map each English function address to the
    Japanese one (build/<cart>/fnmap_*.json); invert it."""
    d = os.path.join(AL, "build", {"IRBJ": "black_ja", "IRAJ": "white_ja"}[code])
    inv = {}
    for f in os.listdir(d):
        if f.startswith("fnmap_") and f.endswith(".json"):
            for en_a, tg_a in json.load(open(os.path.join(d, f))).items():
                inv[int(tg_a) & ~1] = int(en_a) & ~1
    return inv


def find_by_shape(img, en_ptrs):
    """Find the table in a cartridge without a language map (Japanese).

    The count word sits 0x54 bytes before the table, as in English. The
    table is found by its empty (NULL) slots, which are the unused opcodes.
    Japanese has 4 fewer commands: English 0x60 to 0x63 have no Japanese
    entry, so Japanese opcode k is English k below 0x60 and English k+4 from
    0x60 on. The gaps between neighbouring handler addresses (handler sizes)
    are compared to confirm it."""
    base, size = img.ov010
    data = img.segs[1][1]
    en_zero = [k for k, p in enumerate(en_ptrs) if p == 0]
    for off in range(0x54, size - 4, 4):
        cnt = struct.unpack_from("<I", data, off - 0x54)[0]
        if not 500 <= cnt <= 700 or off + 4 * cnt > size:
            continue
        ptrs = [struct.unpack_from("<I", data, off + 4 * k)[0] for k in range(cnt)]
        shift = len(en_ptrs) - cnt
        en_idx = [k if k < 0x60 else k + shift for k in range(cnt)]
        zeros = [k for k, p in enumerate(ptrs) if p == 0]
        if zeros != [k if k < 0x60 else k - shift for k in en_zero]:
            continue
        gaps_same = sum(1 for k in range(cnt - 1)
                        if ptrs[k] and ptrs[k + 1] and en_ptrs[en_idx[k]] and en_ptrs[en_idx[k + 1]]
                        and ptrs[k + 1] - ptrs[k] == en_ptrs[en_idx[k + 1]] - en_ptrs[en_idx[k]])
        print(f"   table found by its empty slots; {shift} fewer commands; "
              f"{gaps_same} of {cnt - 1} neighbour gaps equal English under the 0x60 shift")
        return base + off, cnt, ptrs, [en_ptrs[i] for i in en_idx]
    raise SystemExit("command table not found")


if __name__ == "__main__":
    main()
