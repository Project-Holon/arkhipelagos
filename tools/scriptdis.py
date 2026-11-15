"""Research disassembler for field scripts (a/0/5/7), and a corpus checker.

The real tool is the Rust crate (crates/script); this one is for settling
the command table against all 12 cartridges' scripts.

File layout (aletheia ov010/script_sys.c; BWAP's notes agree):
- a table of u32 offsets, each relative to the end of its own entry, ended
  by the u16 0xFD13;
- code: commands (u16 opcode, then arguments), reached from the table;
- data reached from code: movement lists (4-byte entries, action 0xFE ends).

Decoding follows control flow from every entry. Relative targets are
measured from the end of their own u32 (VM_Jump(vm, vm->pc + offset)).

usage:
  python tools/scriptdis.py check            all 12 cartridges, error summary
  python tools/scriptdis.py score            score disputed opcodes' layouts
  python tools/scriptdis.py dump CART INDEX  listing of one script file
"""
import json
import os
import struct
import sys
from collections import Counter, defaultdict

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import carts  # noqa: E402

SIZE = {"u8": 1, "u16": 2, "wv": 2, "wp": 2, "u32": 4}
MAGIC = 0xFD13


def narc_files(path):
    d = open(path, "rb").read()
    off = struct.unpack_from("<H", d, 0x0C)[0]
    btaf_size, count = struct.unpack_from("<IH", d, off + 4)
    spans = [struct.unpack_from("<II", d, off + 12 + 8 * i) for i in range(count)]
    off += btaf_size
    off += struct.unpack_from("<I", d, off + 4)[0]
    base = off + 8
    return [d[base + s:base + e] for s, e in spans]


class Table:
    """opcode -> (arg kinds, rel flags, ends, name); from commands.json."""

    def __init__(self, fam, overrides=None, path=None):
        path = path or os.path.join(ROOT, "out", "script", "commands_derived.json")
        cmds = json.load(open(path))[fam]
        self.cmds = {}
        for c in cmds:
            if c.get("empty"):
                continue
            kinds = [a["kind"] for a in c["args"]]
            rel = [a.get("rel", False) for a in c["args"]]
            tgt = [a.get("target") for a in c["args"]]
            self.cmds[c["op"]] = {"kinds": kinds, "rel": rel, "target": tgt, "ends": c.get("ends", False),
                                  "name": c.get("name"), "data": DATA_FORMAT.get(c.get("name"))}
        for op, v in (overrides or {}).items():
            self.cmds[op] = dict(self.cmds.get(op, {"name": None, "ends": False, "data": None}), **v)
        self.count = len(cmds)


# data formats reached through a relative pointer, by handler
DATA_FORMAT = {"SCRCMD_FLDMMDL_StartMmdlAnime": "movement", "SCRCMD_POKEMON_FLD_SetWarpTable": "warp"}


def header(data):
    entries, p = [], 0
    while p + 2 <= len(data) and struct.unpack_from("<H", data, p)[0] != MAGIC:
        if p + 4 > len(data):
            return entries, None
        rel = struct.unpack_from("<i", data, p)[0]
        entries.append(p + 4 + rel)
        p += 4
    return entries, p + 2


def decode(data, t, jump_ops, data_ops):
    """Returns (instructions {pos: (op, [(kind, value)], size)}, data blocks
    {pos: size}, errors [(pos, text)], header end)."""
    entries, hend = header(data)
    errs = []
    if hend is None:
        return {}, {}, [(0, "no header end")], None
    ins, blocks = {}, {}
    work = [e for e in entries]
    seen = set()
    while work:
        pc = work.pop()
        while True:
            if pc in ins:
                break
            if pc < hend or pc + 2 > len(data):
                errs.append((pc, "target outside code"))
                break
            op = struct.unpack_from("<H", data, pc)[0]
            c = t.cmds.get(op)
            if c is None:
                errs.append((pc, f"bad opcode {op:#x}"))
                break
            p = pc + 2
            args = []
            ok = True
            for k, rel, tg in zip(c["kinds"], c["rel"], c.get("target", [None] * len(c["kinds"]))):
                n = SIZE.get(k)
                if n is None or p + n > len(data):
                    ok = False
                    break
                v = int.from_bytes(data[p:p + n], "little", signed=(rel and n == 4))
                p += n
                args.append((k, v))
                if rel:
                    target = p + v
                    if tg == "data":
                        if target not in blocks:
                            fmt = c.get("data")
                            if fmt is None:
                                errs.append((pc, f"data target of {op:#x} has no known format"))
                            else:
                                blocks[target] = DATA_SIZERS[fmt](data, target, errs)
                    else:
                        work.append(target)
            if not ok:
                errs.append((pc, f"args run past the end ({op:#x})"))
                break
            ins[pc] = (op, args, p - pc)
            if c["ends"] or op in jump_ops:
                break
            pc = p
    # overlaps: an instruction starting inside another
    starts = sorted(ins)
    for a, b in zip(starts, starts[1:]):
        if a + ins[a][2] > b:
            errs.append((b, f"overlaps instruction at {a:#x}"))
    return ins, blocks, errs, hend


def warp_block(data, pos, errs):
    """Warp list (SCRCMD_POKEMON_FLD_SetWarpTable): rows of 5 u16 (label,
    map, x, y, z), at most 8, ended by a label of 0xFFFF
    (SCRCMD_POKEMON_FLD_CountWarpEntries). The end row's label is counted."""
    for i in range(9):
        q = pos + 10 * i
        if q + 2 > len(data):
            break
        if struct.unpack_from("<H", data, q)[0] == 0xFFFF:
            return 10 * i + 2
    errs.append((pos, "warp list without an end"))
    return 0


def move_block(data, pos, errs):
    p = pos
    while p + 4 <= len(data):
        a = struct.unpack_from("<H", data, p)[0]
        p += 4
        if a == 0xFE:
            return p - pos
    errs.append((pos, "movement list runs past the end"))
    return len(data) - pos


def parse_zone_table(data):
    """The other kind of a/0/5/7 member: a zone's script triggers.
    Entries: u16 type, then
      1: u32 offset (relative to its end) to a list of (work u16, value u16, script u16), ended by u16 0
      2, 3, 4: u16 script id, u16 0
      0: end of the entries.
    Returns (entries, lists) or None when the bytes do not fit."""
    p, entries, lists = 0, [], {}
    while True:
        if p + 2 > len(data):
            return None
        typ = struct.unpack_from("<H", data, p)[0]
        p += 2
        if typ == 0:
            break
        if typ == 1:
            if p + 4 > len(data):
                return None
            off = struct.unpack_from("<i", data, p)[0]
            p += 4
            target = p + off
            entries.append((1, target))
            lists[target] = None
        elif typ in (2, 3, 4):
            if p + 4 > len(data):
                return None
            sid, zero = struct.unpack_from("<HH", data, p)
            p += 4
            entries.append((typ, sid, zero))
        else:
            return None
    end_entries = p
    for target in list(lists):
        q, rows = target, []
        while True:
            if q + 2 > len(data) or q < end_entries:
                return None
            w = struct.unpack_from("<H", data, q)[0]
            if w == 0:
                q += 2
                break
            if q + 6 > len(data):
                return None
            rows.append(struct.unpack_from("<HHH", data, q))
            q += 6
        lists[target] = (rows, q)
    return entries, lists


def kind_of(data):
    """'script', 'zone', 'empty' or 'unknown'."""
    if len(data) == 0:
        return "empty"
    ent, hend = header(data)
    if hend is not None and ent and all(hend <= e < len(data) for e in ent):
        return "script"
    if parse_zone_table(data) is not None:
        return "zone"
    return "unknown"


DATA_SIZERS = {"movement": lambda d, p, e: move_block(d, p, e), "warp": lambda d, p, e: warp_block(d, p, e)}


def fam_of(code):
    return "ja" if code[3] == "J" else "eu"


def ops_for(fam):
    """Jump and movement opcodes per family (named in the table)."""
    t = Table(fam)
    jump = {op for op, c in t.cmds.items() if c["name"] in ("SCRIPT_WORK_CmdJump",)}
    data = {op for op, c in t.cmds.items() if c["name"] in ("SCRCMD_FLDMMDL_StartMmdlAnime",)}
    return jump, data


def corpus():
    C = carts.load()
    for code in carts.CODES:
        yield code, narc_files(os.path.join(C[code]["data"], "a", "0", "5", "7"))


def check(overrides=None, quiet=False, cache=None):
    total = Counter()
    blame = Counter()
    for code, files in (cache or corpus()):
        fam = fam_of(code)
        t = Table(fam, overrides.get(fam) if overrides else None)
        jump, dataops = ops_for(fam)
        for i, f in enumerate(files):
            k = kind_of(f)
            total[k] += 1
            if k != "script":
                continue
            ins, blocks, errs, hend = decode(f, t, jump, dataops)
            covered = sum(s for _, _, s in ins.values()) + sum(blocks.values()) + (hend or 0)
            total["files"] += 1
            total["bytes"] += len(f)
            total["covered"] += covered
            if errs:
                total["files_with_errors"] += 1
                total["errors"] += len(errs)
                for pos, msg in errs:
                    prev = max((p for p in ins if p < pos), default=None)
                    if prev is not None:
                        blame[ins[prev][0], fam] += 1
    if not quiet:
        print(f"files {total['files']}, with errors {total['files_with_errors']}, errors {total['errors']}, "
              f"bytes covered {total['covered']} of {total['bytes']} ({100 * total['covered'] / max(1, total['bytes']):.2f}%)")
        print("most blamed (previous instruction's opcode):", [(hex(o), f, n) for (o, f), n in blame.most_common(15)])
    return total, blame


def dump(code, idx):
    C = carts.load()
    files = narc_files(os.path.join(C[code]["data"], "a", "0", "5", "7"))
    f = files[idx]
    fam = fam_of(code)
    t = Table(fam)
    jump, dataops = ops_for(fam)
    ins, blocks, errs, hend = decode(f, t, jump, dataops)
    for pos in sorted(ins):
        op, args, size = ins[pos]
        print(f"{pos:05x}  {op:04x} {t.cmds[op]['name']}  " + " ".join(f"{k}:{v:#x}" for k, v in args))
    for pos, n in sorted(blocks.items()):
        print(f"{pos:05x}  movement {n} bytes")
    for e in errs:
        print("ERROR", hex(e[0]), e[1])


def bwap_table(path):
    import importlib.util
    spec = importlib.util.spec_from_file_location("sc", path)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    out = {}
    for op, v in m.commands_table.items():
        kinds = [{"1": "u8", "2": "u16", "4": "u32"}[x[0]] for x in v[1:]]
        rel = [x == "4i" and op != 0x15 for x in v[1:]]
        target = [("data" if op in (0x64, 0x1CB) else "code") if r else None for r in rel]
        out[op] = {"kinds": kinds, "rel": rel, "target": target}
    return out


def en_to_ja(op):
    """English opcode -> Japanese opcode (cmdtable.json), or None."""
    t = json.load(open(os.path.join(ROOT, "out", "script", "cmdtable.json")))
    en = {e["en_addr"]: e["op"] for e in t["IRBJ"]["entries"] if e["en_addr"]}
    a = t["IRBO"]["entries"][op]["addr"]
    return en.get(a)


def score(bwap_path):
    """For each opcode whose derived layout differs from BWAP's, decode the
    whole corpus with each layout and keep the one with fewer errors."""
    cache = list(corpus())
    base, _ = check(quiet=True, cache=cache)
    print(f"derived table: {base['errors']} errors, {base['covered']} bytes covered")
    bw = bwap_table(bwap_path)
    d = json.load(open(os.path.join(ROOT, "out", "script", "commands_derived.json")))
    results = {}
    for c in d["eu"]:
        op = c["op"]
        if c.get("empty") or op not in bw:
            continue
        ours = [a["kind"] for a in c["args"]]
        if [SIZE[k] for k in ours] == [SIZE[k] for k in bw[op]["kinds"]]:
            continue
        ov = {"eu": {op: bw[op]}}
        jop = en_to_ja(op)
        if jop is not None:
            ov["ja"] = {jop: bw[op]}
        tot, _ = check(ov, quiet=True, cache=cache)
        better = tot["errors"] < base["errors"]
        results[op] = {"name": c["name"], "derived": ours, "bwap": bw[op]["kinds"],
                       "errors_derived": base["errors"], "errors_bwap": tot["errors"], "bwap_better": better}
        print(f"{op:#05x} {c['name']}: derived {ours} vs BWAP {bw[op]['kinds']}: errors {base['errors']} -> {tot['errors']}"
              f"{'  BWAP better' if better else ''}")
    json.dump(results, open(os.path.join(ROOT, "out", "score.json"), "w"), indent=1)


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "check":
        check()
    elif cmd == "score":
        score(sys.argv[2])
    elif cmd == "dump":
        dump(sys.argv[2], int(sys.argv[3]))
