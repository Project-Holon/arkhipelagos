"""One-off: in data/world/rules.json, turn rules that name a species as an
item ({"has": "Celebi"}) into species tests ({"species": [251, 0]}).

usage: python tools/fix_species_rules.py
"""
import json
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SPECIES = {
    "Celebi": [251, 0], "Raikou": [243, 0], "Entei": [244, 0], "Suicune": [245, 0],
    "Shaymin": [492, 0], "Tornadus": [641, 0], "Thundurus": [642, 0], "Genesect": [649, 0],
    "Deerling (Spring)": [585, 0], "Deerling (Summer)": [585, 1],
    "Deerling (Autumn)": [585, 2], "Deerling (Winter)": [585, 3],
}


def walk(v):
    if isinstance(v, dict):
        if set(v) == {"has"} and v["has"] in SPECIES:
            return {"species": SPECIES[v["has"]]}
        return {k: walk(x) for k, x in v.items()}
    if isinstance(v, list):
        return [walk(x) for x in v]
    return v


def main():
    p = os.path.join(ROOT, "data", "world", "rules.json")
    d = json.load(open(p, encoding="utf-8"))
    before = json.dumps(d)
    d = walk(d)
    n = sum(json.dumps(d).count(f'"species": {json.dumps(s)}') for s in SPECIES.values())
    with open(p, "w", encoding="utf-8", newline="\n") as f:
        json.dump(d, f, indent=1, ensure_ascii=False)
        f.write("\n")
    print("changed" if json.dumps(d) != before else "no change", n)


if __name__ == "__main__":
    main()
