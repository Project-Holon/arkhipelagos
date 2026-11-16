"""Find, for every location, the place in the game's scripts that gives its
item, in all 12 cartridges, and write data/world/item_sites.json.

usage: python tools/locsites.py [--write]

Kinds of site (how):
- ball: an item ball. Map object with script 7000 + e (member 864, entry e):
  the entry loads the item into work 0x800c and 0x800d and calls the shared
  routine L23e2, which sets the object's own flag (GetMmdlEventFlag). The
  flag comes from the object in the zone's event file (a/1/2/5).
- hidden: a hidden item. Background event with script 8000 + e (member 865,
  entry e; 865 has no 0xFD13 after its entry table): the entry loads item,
  count and flag into 0x8000-0x8002 and calls script 2811 (862, L01ae).
- routine: an NPC gift through script 2800 (862, L003e), with the flag in
  work 0x8002 as a constant.
- give: an item given through a shared routine without a flag (LoadWork
  0x8000 item, LoadWork 0x8001 count, CallScript 2801/2802/2805/2806/2807/
  2814): the mod's edit sets the location's flag at that place.
  For the original game's own flags (SetEventFlag after the give) the give
  is found by walking back from the SetEventFlag. For the flags PokemonBWAP
  added (its patched scripts set them; the original game never does) the
  give is found in BWAP's patched script and matched to the original
  script line by line (needs BWAP's decoded base patch, BWAP_BASE below).
- trainer: a trainer who gives an item after the battle. All of them use
  the shared trainer script (863, L0c01: GetTrainerBattleParam into 0x8000,
  CallScript 2805); the flag is the trainer's defeated flag 1420 + trainer.
- badge: SetBadge N in the gym leader's script; the location flag is
  BWAP's 370 + N. The gym's TM shares the flag as gift 8.

The location id is 0x10000 + (flag << 4) + n, with BWAP's flag. Where the
game sets another flag (14 hidden items BWAP numbered on its own), the
site's game_flag says which; the mod checks by game flag.
"""
import difflib
import json
import os
import re
import struct
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import carts  # noqa: E402
from textcodec import narc_files  # noqa: E402

TARGET = os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target"))
BIN = os.path.join(TARGET, "release", "arkh-script")
CACHE = os.path.join(ROOT, "out", "listings")
BWAP_BASE = os.environ.get("BWAP_BASE", "E:/dev/research/bwap/eidos-port/out/base_en/a/0/5/7")

# Shared give routines without a flag (script ids; 862 entries 1, 2, 5, 6, 7, 14).
GIVERS = {0xaf1, 0xaf2, 0xaf5, 0xaf6, 0xaf7, 0xafe}
ROUTINE_FLAG = 0xaf0    # 2800, 862 L003e: flag in 0x8002
HIDDEN_ROUTINE = 0xafb  # 2811, 862 L01ae
TRAINER_BASE = 1420
BADGE_BASE = 370
MARK_WORKS = ("0x805e", "0x805f")

# The 14 hidden items BWAP gave its own numbers to: BWAP flag -> the flag the
# game's hidden-item entry (865) sets. Paired by the zone event file that
# holds the background event, and by order inside the file (flags rise with
# the "#" number, west to east): each game flag below is used by no other
# location, and its event file is the location's zone.
# Locations whose rule asks for a per-world pick (has_value) while the
# mod does not yet change the script to ask for the world's pick: a world
# must not count on them.
PICK_PENDING = {
    444: "its rule needs the world's Driftveil TM pick; the script does not ask for it yet",
}

HIDDEN_GAME_FLAG = {
    438: 1020,                       # Route 18 lower coast (event file 387)
    446: 954, 445: 956,              # Route 6: behind the lab #1 (x 128), east (x 150); #2 is 955
    451: 982,                        # Route 9 (file 348)
    459: 997, 460: 998, 461: 1000,   # Route 12 #1, #2, #4 (file 368; #3 is 999)
    453: 1009, 454: 1010,            # Undella Bay western shore (file 240)
    455: 1003, 456: 1004, 457: 1005,  # Route 13 south eastern coast (file 370)
    448: 1024, 449: 1025,            # Icirrus City south, winter (file 113)
}


def fam(code):
    return "ja" if code.endswith("J") else "eu"


def listing(code, member):
    p = os.path.join(CACHE, code, "%03d.txt" % member)
    if os.path.exists(p):
        return open(p, encoding="utf-8").read()
    C = carts.load()
    r = subprocess.run([BIN, "dis", fam(code), os.path.join(C[code]["data"], "a", "0", "5", "7"), str(member)],
                       capture_output=True, text=True)
    text = r.stdout if r.returncode == 0 else ""
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    return text


class Listing:
    """A member's listing: entries, and its lines with the command ordinal
    (the index among commands, as the script crate counts Item::Cmd)."""

    def __init__(self, text):
        self.entries = []
        self.lines = []  # (kind, text, cmd ordinal or None)
        n = 0
        for line in text.splitlines():
            if line.startswith(".entries"):
                self.entries = line.split()[1:]
            elif line.endswith(":") and not line.startswith(" "):
                self.lines.append(("label", line[:-1], None))
            elif line.startswith("    ") and not line.strip().startswith("."):
                self.lines.append(("cmd", line.strip(), n))
                n += 1
            else:
                self.lines.append(("other", line, None))

    def cmd(self, i):
        if i is None or not 0 <= i < len(self.lines):
            return None
        k, t, n = self.lines[i]
        return t.split() if k == "cmd" else None


def num(s):
    return int(s, 16) if s.startswith("0x") else None


def give_at(L, i):
    """A give pattern starting at line i: (item, count, routine, ordinal of
    the first command, ordinal after the CallScript), else None."""
    c = [L.cmd(j) for j in range(i, i + 3)]
    if None in c:
        return None
    a, b, s = c
    if not (a[0].startswith("SCRIPT_WORK_CmdLoadWork") and a[1] == "0x8000" and num(a[2]) is not None):
        return None
    if not (b[0].startswith("SCRIPT_WORK_CmdLoadWork") and b[1] == "0x8001" and num(b[2]) is not None):
        return None
    if s[0] != "SCRIPT_WORK_CmdCallScript" or num(s[1]) not in GIVERS:
        return None
    return num(a[2]), num(b[2]), num(s[1]), L.lines[i][2], L.lines[i + 2][2] + 1


def gives_before(L, i, locflags, own):
    """Walk back from line i (a SetEventFlag) and list the gives of its
    block: stop at an entry label, an End or Halt, or another location's
    SetEventFlag."""
    out = []
    j = i - 1
    while j >= 0 and i - j < 80:
        k, t, _ = L.lines[j]
        if k == "label" and t in L.entries:
            break
        c = L.cmd(j)
        if c:
            if c[0] in ("SCRIPT_WORK_CmdEnd", "SCRIPT_WORK_CmdHalt"):
                break
            if c[0] == "SCRIPT_WORK_CmdSetEventFlag" and num(c[1]) in locflags and num(c[1]) != own:
                break
            g = give_at(L, j)
            if g:
                out.append((j, g))
        j -= 1
    out.reverse()
    return out


def norm(L):
    """Lines for matching BWAP's patched listing to the original one."""
    out = []
    for k, t, _ in L.lines:
        if k == "label":
            out.append("L:")
        elif k == "cmd":
            t = re.sub(r"@L[0-9a-f]+", "@", t)
            t = re.sub(r"^(SCRIPT_WORK_CmdLoadWork\w+ 0x8000) 0x[0-9a-f]+$", r"\1 *", t)
            out.append(t)
        else:
            out.append(t)
    return out


def align(orig, patched):
    """patched line index -> original line index, where they match."""
    a, b = norm(orig), norm(patched)
    m = {}
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
        if tag == "equal" or (tag == "replace" and i2 - i1 == j2 - j1):
            for d in range(j2 - j1):
                if tag == "equal" or a[i1 + d] == b[j1 + d]:
                    m[j1 + d] = i1 + d
    return m


def event_files(code):
    C = carts.load()
    return narc_files(os.path.join(C[code]["data"], "a", "1", "2", "5"))


def balls(code):
    """object flag -> (script id, event file, object index). Event file:
    u32 size, four counts (bg, npc, connect, pos), then bg events (0x14),
    objects (0x24: id, code, move, event type, flag, script; MmdlHeader)."""
    out = {}
    for fi, m in enumerate(event_files(code)):
        if len(m) < 8:
            continue
        nb, nn = m[4], m[5]
        p = 8 + 0x14 * nb
        for i in range(nn):
            _oid, _code, _move, _etype, flag, script = struct.unpack_from("<6H", m, p + 0x24 * i)
            if 7000 <= script < 7800:
                out.setdefault(flag, []).append((script, fi, i))
    return out


def ball_entries(code):
    """864 entry -> (item, count), each checked to call the shared routine."""
    L = Listing(listing(code, 864))
    lab = {t: i for i, (k, t, _) in enumerate(L.lines) if k == "label"}
    out = {}
    for e, name in enumerate(L.entries):
        i = lab[name]
        c = [L.cmd(i + d) for d in range(1, 5)]
        if (None not in c and c[0][0] == "SCRIPT_WORK_CmdCallFieldCommand" and c[1][1] == "0x800c"
                and c[2][1] == "0x800d" and c[3][0] == "SCRIPT_WORK_CmdCall"):
            out[e] = (num(c[1][2]), num(c[2][2]), c[3][1])
    return out


def hidden_entries(code):
    """865: game flag -> (entry, item, count). The member has no 0xFD13: the
    entry table ends where the first entry's code starts."""
    C = carts.load()
    m = narc_files(os.path.join(C[code]["data"], "a", "0", "5", "7"))[865]
    first = 4 + struct.unpack_from("<I", m, 0)[0]
    out = {}
    for e in range(first // 4):
        t = 4 * e + 4 + struct.unpack_from("<i", m, 4 * e)[0]
        w = struct.unpack_from("<15H", m, t)
        ok = (w[0] == 0x2e and w[1] == 0x2a and w[2] == 0x8000 and w[4] == 0x2a and w[5] == 0x8001
              and w[7] == 0x2a and w[8] == 0x8002 and w[10] == 0x1c and w[11] == HIDDEN_ROUTINE)
        if not ok:
            raise SystemExit(f"{code}: 865 entry {e} is not a hidden item")
        out[w[9]] = (e, w[3], w[6])
    return out


def scan(code, members, locflags):
    """Per member: routine gifts (0x8002 constant before script 2800), gives
    followed by the game's own SetEventFlag, SetBadge sites, CheckBadge
    sites, the trainer gift block, and uses of the marker works."""
    res = {"routine": {}, "set": {}, "badge": {}, "checkbadge": {}, "trainer": [], "marks": 0}
    for mem in members:
        text = listing(code, mem)
        res["marks"] += sum(len(re.findall(r"\b%s\b" % w, text)) for w in MARK_WORKS)
        L = Listing(text)
        for i, (k, t, n) in enumerate(L.lines):
            if k != "cmd":
                continue
            c = t.split()
            if len(c) > 2 and c[0].startswith("SCRIPT_WORK_CmdLoadWork") and c[1] == "0x8002" and num(c[2]) in locflags:
                for j in range(i + 1, min(i + 6, len(L.lines))):
                    d = L.cmd(j)
                    if d and d[0] == "SCRIPT_WORK_CmdCallScript":
                        if num(d[1]) == ROUTINE_FLAG:
                            res["routine"].setdefault(num(c[2]), []).append({"member": mem, "cmd": n})
                        break
            elif c[0] == "SCRIPT_WORK_CmdSetEventFlag" and num(c[1]) in locflags:
                f = num(c[1])
                g = gives_before(L, i, locflags, f)
                if g:
                    res["set"].setdefault(f, []).append({"member": mem, "gives": [x[1] for x in g]})
            elif c[0] == "SCRCMD_WORK_CmdSetBadge":
                res["badge"].setdefault(num(c[1]), []).append({"member": mem, "cmd": n})
            elif c[0] == "SCRCMD_WORK_CmdCheckBadge":
                res["checkbadge"].setdefault(mem, []).append({"cmd": n, "badge": num(c[2]), "out": c[1]})
            elif mem == 863 and c[0] == "SCRCMD_GetTrainerBattleParam" and c[2] == "0x8000":
                g = [L.cmd(i + 1), L.cmd(i + 2)]
                if g[0] and g[0][1] == "0x8001" and g[1] and g[1][0] == "SCRIPT_WORK_CmdCallScript":
                    res["trainer"].append({"member": mem, "cmd": L.lines[i + 1][2], "resume": L.lines[i + 2][2] + 1})
    return res


def bwap_gives(locflags, own_flags):
    """BWAP's patched scripts: flag -> list of (member, gives in the original
    listing). Only for the flags the original game never sets."""
    out = {}
    if not os.path.isdir(BWAP_BASE):
        print("BWAP's decoded base patch is not at", BWAP_BASE)
        return out
    for fn in os.listdir(BWAP_BASE):
        mem = int(fn)
        r = subprocess.run([BIN, "dis-file", "eu", os.path.join(BWAP_BASE, fn)], capture_output=True, text=True)
        if r.returncode != 0:
            continue
        P = Listing(r.stdout)
        O = None
        m = None
        for i, (k, t, _) in enumerate(P.lines):
            if k != "cmd":
                continue
            c = t.split()
            if c[0] == "SCRIPT_WORK_CmdSetEventFlag" and num(c[1]) in own_flags:
                if m is None:
                    O = Listing(listing("IRBO", mem))
                    m = align(O, P)
                    pg = [j for j in range(len(P.lines)) if give_at(P, j)]
                    og = [j for j in range(len(O.lines)) if give_at(O, j)]
                gs = []
                for j, _g in gives_before(P, i, locflags, num(c[1])):
                    oj = m.get(j)
                    g = give_at(O, oj) if oj is not None else None
                    if g is None and len(pg) == len(og):
                        # the same give, counted in order inside the member
                        g = give_at(O, og[pg.index(j)])
                        g = g + ("by order",) if g else None
                    gs.append(g)
                out.setdefault(num(c[1]), []).append({"member": mem, "gives": gs})
    return out


def main():
    locs = json.load(open(os.path.join(ROOT, "data", "world", "locations.json"), encoding="utf-8"))["locations"]
    byflag = {}
    for l in locs:
        byflag.setdefault(l["flag"], []).append(l)
    locflags = set(byflag)
    members = list(range(899))

    with ThreadPoolExecutor(8) as ex:
        for code in carts.CODES:
            list(ex.map(lambda m, code=code: listing(code, m), members))

    per = {}
    for code in carts.CODES:
        per[code] = {"balls": balls(code), "ball_entries": ball_entries(code), "hidden": hidden_entries(code),
                     "scan": scan(code, members, locflags)}
        print(code, "uses of works 0x805e and 0x805f:", per[code]["scan"]["marks"])
    ref = per["IRBO"]

    def same(key):
        return [c for c in carts.CODES if per[c][key] != ref[key]]

    def same_scan(key):
        return [c for c in carts.CODES if per[c]["scan"][key] != ref["scan"][key]]

    print("differ from English Black: balls", same("balls"), "ball entries", same("ball_entries"),
          "hidden", same("hidden"))
    for key in ("routine", "set", "badge", "trainer", "checkbadge"):
        print("  ", key, same_scan(key))

    game_set = set(ref["scan"]["set"]) | set(ref["scan"]["routine"]) | set(ref["hidden"]) | set(ref["balls"])
    own_flags = {f for f in locflags if f not in game_set and f < TRAINER_BASE}
    bw = bwap_gives(locflags, own_flags)

    sites = {}
    why = {}
    for l in locs:
        f, kind = l["flag"], l["kind"]
        n = l["id"] - 0x10000 - (f << 4)
        lid = str(l["id"])
        s = None
        if kind == "item_ball" and f in ref["balls"]:
            script, fi, oi = ref["balls"][f][0]
            item, count, _ = ref["ball_entries"][script - 7000]
            s = {"how": "ball", "script": script, "event_file": fi, "object": oi, "item": item, "count": count}
        elif kind == "hidden":
            gf = HIDDEN_GAME_FLAG.get(f, f)
            if gf in ref["hidden"]:
                e, item, count = ref["hidden"][gf]
                s = {"how": "hidden", "entry": e, "item": item, "count": count}
                if gf != f:
                    s["game_flag"] = gf
        elif kind == "badge" and (f - BADGE_BASE) in ref["scan"]["badge"]:
            s = {"how": "badge", "badge": f - BADGE_BASE, "sites": [dict(x) for x in ref["scan"]["badge"][f - BADGE_BASE]]}
        elif f >= TRAINER_BASE and ref["scan"]["trainer"]:
            s = {"how": "trainer", "trainer": f - TRAINER_BASE}
        elif f in ref["scan"]["routine"] and n == 0:
            s = {"how": "routine", "sites": [dict(x) for x in ref["scan"]["routine"][f]]}
        if s is None and kind == "tm" and n == 8 and (f - BADGE_BASE) in ref["scan"]["badge"]:
            # the gym's TM: the give of a TM in the leader's member
            mems = {x["member"] for x in ref["scan"]["badge"][f - BADGE_BASE]}
            found = []
            for mem in sorted(mems):
                L = Listing(listing("IRBO", mem))
                for i in range(len(L.lines)):
                    g = give_at(L, i)
                    if g and 328 <= g[0] <= 427:
                        found.append({"member": mem, "cmd": g[3], "resume": g[4], "item": g[0],
                                      "count": g[1], "routine": g[2]})
            if found:
                s = {"how": "give", "from": "gym", "sites": found}
        if s is None and kind not in ("hidden", "badge"):
            cands, src = [], None
            if f in ref["scan"]["set"]:
                cands, src = [(x["member"], x["gives"]) for x in ref["scan"]["set"][f]], "game"
            elif f in bw:
                cands, src = [(x["member"], x["gives"]) for x in bw[f]], "bwap"
            ids = sorted(x["id"] for x in byflag[f])
            k = len(ids)
            mine = ids.index(l["id"])
            found = []
            bad = None
            for mem, gs in cands:
                if any(g is None for g in gs):
                    bad = "a give in BWAP's script has no matching line in the original"
                    continue
                if k == 1:
                    found += [(mem, g, n) for g in gs]
                elif len(gs) == k:
                    found.append((mem, gs[mine], n))
                elif sum(g[1] for g in gs) == k:
                    # one give of c items stands for c locations (BWAP counts
                    # each ball of "10 Poke Balls"); its key is the first one's n
                    first = 0
                    for g in gs:
                        if first <= mine < first + g[1]:
                            found.append((mem, g, ids[first] - 0x10000 - (f << 4)))
                        first += g[1]
                else:
                    bad = f"{len(gs)} gives for {k} locations"
            if found:
                s = {"how": "give", "from": src, "key": found[0][2], "sites": [
                    dict({"member": mem, "cmd": g[3], "resume": g[4], "item": g[0], "count": g[1], "routine": g[2]},
                         **({"matched": g[5]} if len(g) > 5 else {}))
                    for mem, g, _ in found]}
            else:
                why[lid] = bad or ("the flag is set nowhere in the original scripts nor in BWAP's"
                                   if not cands else "no give before the flag")
        if s is not None and f in PICK_PENDING:
            why[lid] = PICK_PENDING[f]
            s = None
        if s is None and lid not in why:
            why[lid] = "no site found"
        if s is not None:
            s["flag"] = f
            s["n"] = n
            s.setdefault("key", n if s["how"] in ("give", "badge") else 0)
            sites[lid] = s
    # Command ordinals per family: the Japanese scripts differ a little, so
    # each site is found again there as the same command (the same
    # occurrence of it in the member), then checked in every cartridge.
    cmds = {}

    def cmd_texts(code, mem):
        if (code, mem) not in cmds:
            L = Listing(listing(code, mem))
            cmds[(code, mem)] = [t for k, t, _ in L.lines if k == "cmd"]
        return cmds[(code, mem)]

    def per_family(mem, o, span):
        E = cmd_texts("IRBO", mem)
        key = E[o:o + span]
        occ = sum(1 for i in range(o) if E[i:i + span] == key)
        out = {"eu": o}
        J = cmd_texts("IRBJ", mem)
        hits = [i for i in range(len(J)) if J[i:i + span] == key]
        if occ >= len(hits):
            raise SystemExit(f"member {mem} command {o}: not found in the Japanese script")
        out["ja"] = hits[occ]
        for code in carts.CODES:
            ref_code = "IRBJ" if fam(code) == "ja" else "IRBO"
            got = cmd_texts(code, mem)[out[fam(code)]:out[fam(code)] + span]
            if got != cmd_texts(ref_code, mem)[out[fam(code)]:out[fam(code)] + span]:
                raise SystemExit(f"{code} member {mem} command {o}: differs from {ref_code}")
        return out

    # A give matched only by its order in the member loses to a line-matched
    # one at the same place (BWAP added gives of its own in a few scripts).
    claims = {}
    for lid, s in sites.items():
        for x in s.get("sites", []):
            if s["how"] == "give" and "matched" not in x:
                claims[(x["member"], x["cmd"])] = (s["flag"], s["key"])
    for lid in list(sites):
        s = sites[lid]
        if s["how"] == "give" and any("matched" in x and claims.get((x["member"], x["cmd"]), (s["flag"], s["key"]))
                                      != (s["flag"], s["key"]) for x in s["sites"]):
            why[lid] = "BWAP's give has no matching line in the original (its place is another location's)"
            del sites[lid]
    seen = {}
    for lid, s in sites.items():
        for x in s.get("sites", []):
            span = 3 if s["how"] == "give" else 1
            o = per_family(x["member"], x["cmd"], span)
            if "resume" in x:
                x["resume"] = {f2: o[f2] + (x["resume"] - x["cmd"]) for f2 in o}
            x["cmd"] = o
            if s["how"] == "give":
                k2 = (x["member"], o["eu"])
                if seen.setdefault(k2, (s["flag"], s["key"])) != (s["flag"], s["key"]):
                    raise SystemExit(f"site {k2} stands for two checks: {seen[k2]} and {(s['flag'], s['key'])}")
    trainer = [dict(t, cmd=per_family(t["member"], t["cmd"], 1)) for t in ref["scan"]["trainer"]]
    for t, r in zip(trainer, ref["scan"]["trainer"]):
        t["resume"] = {f2: t["cmd"][f2] + (r["resume"] - r["cmd"]) for f2 in t["cmd"]}
    checkbadge = {str(mem): [dict(x, cmd=per_family(mem, x["cmd"], 1)) for x in v]
                  for mem, v in ref["scan"]["checkbadge"].items()}

    count = {}
    for l in locs:
        h = sites[str(l["id"])]["how"] if str(l["id"]) in sites else "none"
        count.setdefault(l["kind"], {}).setdefault(h, 0)
        count[l["kind"]][h] += 1
    for k, v in sorted(count.items()):
        print(f"  {k}: {v}")
    for lid, w in sorted(why.items(), key=lambda x: int(x[0])):
        l = next(x for x in locs if str(x["id"]) == lid)
        print("  none:", l["flag"], l["name"].encode("ascii", "replace").decode(), "-", w)

    if "--write" in sys.argv:
        # routine, badge and trainer sites are the same everywhere; give
        # sites and CheckBadge were checked per family above
        differ = same("balls") + same("ball_entries") + same("hidden") + sum(
            (same_scan(k) for k in ("routine", "badge", "trainer")), [])
        if differ:
            raise SystemExit(f"not written: sites differ in {sorted(set(differ))}")
        p = os.path.join(ROOT, "data", "world", "item_sites.json")
        with open(p, "w", encoding="utf-8", newline="\n") as f:
            json.dump({"_source": "found in the cartridges' scripts and zone event files by tools/locsites.py; "
                                  "the same in all 12. cmd: a command's ordinal in the member's listing",
                       "sites": sites, "none": why, "trainer_gift": trainer, "checkbadge": checkbadge},
                      f, indent=1)
            f.write("\n")
        print("wrote", p)


if __name__ == "__main__":
    main()
