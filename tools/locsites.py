"""Find, for every location, the field scripts that set its event flag, in
all 12 cartridges.

usage: python tools/locsites.py [--write]

A site is (script member, entry, how): "set" for SetEventFlag with the flag
as a constant, "ball" for an item ball (the flag loaded into work 0x8002
before the shared item-ball routine sets it). Hidden items set the flag of
their map object in the shared hidden-item routine, so they have no site
of their own; they are listed as "hidden".

The sites must be the same (member and entry) in every cartridge; --write
then stores them in data/world/item_sites.json.
"""
import json
import os
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import carts  # noqa: E402

TARGET = os.environ.get("CARGO_TARGET_DIR", os.path.join(ROOT, "target"))
BIN = os.path.join(TARGET, "release", "arkh-script")
SET = re.compile(r"^\s+SCRIPT_WORK_CmdSetEventFlag (0x[0-9a-f]+)$")
LOAD = re.compile(r"^\s+SCRIPT_WORK_CmdLoadWork(?:Const|Value) 0x8002 (0x[0-9a-f]+)$")


def dis(code, member):
    C = carts.load()
    fam = "ja" if code.endswith("J") else "eu"
    r = subprocess.run([BIN, "dis", fam, os.path.join(C[code]["data"], "a", "0", "5", "7"), str(member)],
                       capture_output=True, text=True)
    return r.stdout if r.returncode == 0 else ""


def sites_of(text):
    """flag -> set of (entry index, how) in one member's listing."""
    out = {}
    entries = []
    entry_of = {}
    cur = None
    for line in text.splitlines():
        if line.startswith(".entries"):
            entries = line.split()[1:]
            entry_of = {lab: i for i, lab in enumerate(entries)}
            continue
        if line.endswith(":") and not line.startswith(" "):
            lab = line[:-1]
            if lab in entry_of:
                cur = entry_of[lab]
            continue
        for rx, how in ((SET, "set"), (LOAD, "ball")):
            m = rx.match(line)
            if m:
                f = int(m.group(1), 16)
                if f < 0x4000:
                    out.setdefault(f, set()).add((cur, how))
    return out


def cart_sites(code, members):
    res = {}
    with ThreadPoolExecutor(8) as ex:
        for mem, text in zip(members, ex.map(lambda m: dis(code, m), members)):
            for f, s in sites_of(text).items():
                for e, how in s:
                    res.setdefault(f, set()).add((mem, e, how))
    return res


def main():
    locs = json.load(open(os.path.join(ROOT, "data", "world", "locations.json"), encoding="utf-8"))["locations"]
    flags = sorted({l["flag"] for l in locs})
    members = list(range(0, 899))
    per = {}
    for code in carts.CODES:
        per[code] = cart_sites(code, members)
        print(code, "flags with sites:", sum(1 for f in flags if f in per[code]))
    ref = per["IRBO"]
    differ = [code for code in carts.CODES if {f: per[code].get(f) for f in flags} != {f: ref.get(f) for f in flags}]
    print("cartridges whose sites differ from English Black:", differ or "none")
    by_kind = {}
    out = {}
    for l in locs:
        s = sorted(ref.get(l["flag"], set()), key=lambda x: (x[0], -1 if x[1] is None else x[1], x[2]))
        kind = l["kind"]
        found = "hidden" if kind == "hidden" else ("yes" if s else "no")
        by_kind.setdefault(kind, {}).setdefault(found, 0)
        by_kind[kind][found] += 1
        out[str(l["id"])] = {"flag": l["flag"], "kind": kind,
                             "sites": [{"member": m, "entry": e, "how": h} for m, e, h in s]}
    for k, v in sorted(by_kind.items()):
        print(f"  {k}: {v}")
    if "--write" in sys.argv:
        if differ:
            raise SystemExit("not written: the sites differ between cartridges")
        p = os.path.join(ROOT, "data", "world", "item_sites.json")
        with open(p, "w", encoding="utf-8", newline="\n") as f:
            json.dump({"_source": "found in the cartridges' field scripts by tools/locsites.py; the same in all 12",
                       "sites": out}, f, indent=1)
            f.write("\n")
        print("wrote", p)


if __name__ == "__main__":
    main()
