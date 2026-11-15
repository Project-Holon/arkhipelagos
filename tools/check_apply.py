"""Check the apply step on the cartridges: make a world, write each
player's changed archives with arkh-apply on every cartridge of the
player's version, read them back with tools/facts.py's own readers, and
compare with the world's tables (the original facts plus the changes).

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
            subprocess.run([os.path.join(BIN, "arkh-apply"), world, str(p["slot"]), C[code]["data"], out],
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
            print(f"{p['name']} ({p['version']}) {code}: {'ok' if not bad else 'differences'}")
    print("apply matches the world on every cartridge" if not bad else f"{bad} differences")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
