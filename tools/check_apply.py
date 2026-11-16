"""Check the apply step on the cartridges: make a world, write each
player's changed archives with arkh-apply on every cartridge of the
player's version, read them back with tools/facts.py's own readers, and
compare with the world's tables (the original facts plus the changes).

Then the check edits (crates/apply/src/scripts.rs): written on every
cartridge with `arkh-apply scripts`, read back with arkh-script, and each
site in data/world/item_sites.json must now jump to code that loads its key
into 0x805E, the marker into 0x805F, and sets its flag.

usage: python tools/check_apply.py [config.json]   (default: tests/configs/all-random.json)
"""
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import carts  # noqa: E402
import facts  # noqa: E402
from textcodec import narc_files  # noqa: E402

TARGET = os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target"))
BIN = os.path.join(TARGET, "release")


def load_facts(name, version):
    d = json.load(open(os.path.join(ROOT, "data", "facts", f"{name}.json"), encoding="utf-8"))
    return d["all"] if "all" in d else d[version]


def expected(p):
    """The player's tables: original facts with the world's changes on top."""
    v = p["version"]
    t = p.get("tables", {})
    out = {}
    sp = load_facts("species", v)
    for k, rec in t.get("personal", {}).items():
        sp[k] = rec
    out["species"] = sp
    ls = load_facts("learnsets", v)
    for k, x in t.get("learnsets", {}).items():
        ls[k] = x
    out["learnsets"] = ls
    ev = load_facts("evolutions", v)
    for k, x in t.get("evolutions", {}).items():
        if x:
            ev[k] = x
        else:
            ev.pop(k, None)
    out["evolutions"] = ev
    mv = load_facts("moves", v)
    for k, x in t.get("moves", {}).items():
        mv[k] = x
    out["moves"] = mv
    en = load_facts("encounters", v)
    for k, x in t.get("wild", {}).items():
        en[k] = x
    out["encounters"] = en
    tr = load_facts("trainers", v)
    for k, x in t.get("trainers", {}).items():
        tr[k] = x
    out["trainers"] = tr
    return out


def main():
    cfg = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "tests", "configs", "all-random.json")
    world = os.path.join(ROOT, "out", "apply-world.json")
    subprocess.run([os.path.join(BIN, "arkh-gen"), "world", cfg, world], check=True, capture_output=True)
    w = json.load(open(world, encoding="utf-8"))
    C = carts.load()
    bad = 0
    for p in w["players"]:
        want = expected(p)
        for code in carts.CODES:
            if carts.version(code) != p["version"]:
                continue
            out = os.path.join(ROOT, "out", "apply", f"{p['slot']}-{code}")
            fam = "ja" if code.endswith("J") else "eu"
            subprocess.run([os.path.join(BIN, "arkh-apply"), world, str(p["slot"]), C[code]["data"], out, fam],
                           check=True, capture_output=True)

            # facts.py's readers, pointed at the written archives first
            def narc(cart, path, out=out):
                q = os.path.join(out, *path.split("/"))
                if os.path.exists(q):
                    return narc_files(q)
                return narc_files(os.path.join(cart["data"], *path.split("/")))
            facts.narc = narc
            cart = C[code]
            got = {
                "species": facts.species_table(cart),
                "learnsets": facts.learnsets(cart),
                "evolutions": facts.evolutions(cart),
                "moves": facts.moves(cart),
                "encounters": facts.encounters(cart),
                "trainers": facts.trainers(cart),
            }
            for k in got:
                a = json.loads(json.dumps(got[k]))
                b = json.loads(json.dumps(want[k]))
                if a != b:
                    bad += 1
                    keys = [x for x in b if a.get(x) != b.get(x)][:5]
                    print(f"{p['name']} {code} {k}: differs at {keys}")
                    for x in keys[:2]:
                        print("   ours", json.dumps(a.get(x))[:200])
                        print("   want", json.dumps(b.get(x))[:200])
            bad += check_species(p, code, out)
            print(f"{p['name']} ({p['version']}) {code}: {'ok' if not bad else 'differences'}")
    print("apply matches the world on every cartridge" if not bad else f"{bad} differences")
    bad += check_edits()
    sys.exit(1 if bad else 0)


GIVES = ("SCRCMD_PROC_FLD_StartScriptedWildBattle", "SCRCMD_PROC_AddMonToPartySimple",
         "SCRCMD_PROC_AddMonToPartyDetailed", "SCRCMD_PROC_AddEggToParty")


def check_species(p, code, out):
    """The world's statics, gifts, fossils and starters are in the written
    scripts, and its trades in a/1/6/5."""
    import struct
    sys.path.insert(0, HERE)
    data = json.load(open(os.path.join(ROOT, "data", "world", "species.json"), encoding="utf-8"))
    arc = os.path.join(out, "a", "0", "5", "7")
    members = {"Castelia City Gift": 94, "Dreamyard Gift": 304, "Dreamyard Static": 306, "Rumination": 312,
               "Desert Resort": 316, "Relic Castle": 364, "Dragonspiral": 426, "Trial Chamber": 458,
               "Giant Chasm": 468, "Liberty Garden": 474, "Marvelous Bridge": 526, "Guidance": 670,
               "Abundant": 752, "Lostlorn": 770, "Route 18 Egg": 776, "Reanimation": 877,
               "Item Encounter": 897}
    bad = 0
    statics = p["tables"].get("statics", {})
    cache = {}
    for st in data["statics"]:
        new = statics.get(st["name"])
        if not new or st["kind"] == "starter" or st.get("fixed"):
            continue
        m = next((v for k, v in members.items() if k in st["name"]), None)
        if m not in cache:
            cache[m] = dis(code, arc, m)[0]
        loads = [c for c in cache[m] if c[0] in GIVES or (c[0] == "SCRIPT_WORK_CmdLoadWorkConst" and c[1] == "0x8023")]
        if not any(hex(new[0]) in c[1:3] for c in loads):
            bad += 1
            print(f"  {code}: {st['name']} should give {new[0]}")
    if p.get("starters"):
        cmds = dis(code, arc, 782)[0]
        got = [int(c[2], 16) for c in cmds if c[:2] == ["SCRIPT_WORK_CmdLoadWorkConst", "0x8021"] and c[2] != "0x0"]
        if got != p["starters"]:
            bad += 1
            print(f"  {code}: starters {got}, want {p['starters']}")
    picks = p.get("picks", {})
    if picks.get("studio_castelia_type") is not None:
        want = ["SCRIPT_WORK_CmdLoadWorkConst", "0x4137", hex(picks["studio_castelia_type"])]
        if want not in dis(code, arc, 86)[0]:
            bad += 1
            print(f"  {code}: the studio does not ask for type {picks['studio_castelia_type']}")
    if picks.get("driftveil_tm") is not None:
        mach = json.load(open(os.path.join(ROOT, "data", "facts", "machines.json"), encoding="utf-8"))["all"]
        mv = next(x["move"] for x in mach if x["item"] == picks["driftveil_tm"])
        if ["SCRIPT_WORK_CmdLoadWorkConst", "0x413b", hex(mv)] not in dis(code, arc, 212)[0]:
            bad += 1
            print(f"  {code}: the Driftveil lady does not ask for move {mv}")
    trades = p["tables"].get("trades", {})
    if trades:
        recs = narc_files(os.path.join(out, "a", "1", "6", "5"))
        ids = {"Nacrene City Trade Encounter": 0, "Driftveil City Trade Encounter": 2, "Route 7 Trade Encounter": 4,
               "Undella Town Trade Encounter": 5, "Route 15 Trade Encounter": 6}
        for name, (offer, wanted) in trades.items():
            r = recs[ids[name] + (1 if p["version"] == "white" and ids[name] in (0, 2) else 0)]
            got = (struct.unpack_from("<I", r, 4)[0], struct.unpack_from("<I", r, 8)[0], struct.unpack_from("<I", r, 0x5C)[0])
            if got != (offer[0], offer[1], wanted):
                bad += 1
                print(f"  {code}: trade {name} is {got}, want {offer} for {wanted}")
    return bad


def dis(code, arc, member):
    fam = "ja" if code.endswith("J") else "eu"
    r = subprocess.run([os.path.join(BIN, "arkh-script"), "dis", fam, arc, str(member)],
                       capture_output=True, text=True, check=True)
    lines = r.stdout.splitlines()
    cmds, labels = [], {}
    for line in lines:
        if line.endswith(":") and not line.startswith(" "):
            labels[line[:-1]] = len(cmds)
        elif line.startswith("    ") and not line.strip().startswith("."):
            cmds.append(line.split())
    return cmds, labels


def check_edits():
    """Each edited member holds exactly the checks its sites ask for.
    (Where each one sits is checked by arkh-apply itself: the original
    command at each site, and every byte outside the edits kept.)"""
    sites = json.load(open(os.path.join(ROOT, "data", "world", "item_sites.json"), encoding="utf-8"))
    C = carts.load()
    want = {}
    seen = set()
    for lid, s in sites["sites"].items():
        if s["how"] not in ("give", "badge"):
            continue
        for x in s["sites"]:
            k = (x["member"], x["cmd"]["eu"])
            if k in seen:
                continue
            seen.add(k)
            want.setdefault(x["member"], []).append(("SCRIPT_WORK_CmdSetEventFlag", hex(s["flag"]), hex(s["key"])))
    want.setdefault(862, []).extend([("SCRIPT_WORK_CmdSetEventFlag", "0x8002", "0x0")] * 2)
    want.setdefault(864, []).append(("SCRIPT_WORK_CmdSetEventFlag", "0x8010", "0x0"))
    want.setdefault(863, []).extend([("SCRCMD_WORK_CmdSetTrainerDefeatedFlag", "0x8020", "0x0")]
                                    * len(sites["trainer_gift"]))
    bad = 0
    for code in carts.CODES:
        fam = "ja" if code.endswith("J") else "eu"
        out = os.path.join(ROOT, "out", "apply-scripts", code)
        r = subprocess.run([os.path.join(BIN, "arkh-apply"), "scripts", fam, C[code]["data"], out],
                           capture_output=True, text=True)
        if r.returncode != 0:
            print(code, "check edits:", r.stderr.strip())
            bad += 1
            continue
        arc = os.path.join(out, "a", "0", "5", "7")
        for m, w in sorted(want.items()):
            cmds, _ = dis(code, arc, m)
            got = []
            for i in range(len(cmds) - 3):
                a, b, c, d = cmds[i:i + 4]
                if (a[:2] == ["SCRIPT_WORK_CmdLoadWorkConst", "0x805e"] and b == ["SCRIPT_WORK_CmdLoadWorkConst", "0x805f", "0xa4c7"]
                        and d == ["SCRIPT_WORK_CmdLoadWorkConst", "0x805f", "0x0"]):
                    got.append((c[0], c[1], a[2]))
            if sorted(got) != sorted(w):
                bad += 1
                print(f"{code} member {m}: checks {sorted(got)[:4]}, want {sorted(w)[:4]}")
        print(f"{code}: {r.stdout.strip()}; {sum(len(v) for v in want.values())} checks read back")
    print("check edits read back on every cartridge" if not bad else f"{bad} check edit problems")
    return bad


if __name__ == "__main__":
    main()
