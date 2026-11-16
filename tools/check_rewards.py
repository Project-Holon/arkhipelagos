"""Check the badge and TM reward rules (shuffle_badge_rewards,
shuffle_tm_rewards) on worlds the generator makes: 8 seeds of a four-player
world with every mode.

    python tools/check_rewards.py <arkh-gen>"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
gen = sys.argv[1]
items = {i["id"]: i for i in json.loads((ROOT / "data/world/items.json").read_text(encoding="utf-8"))["items"]}
locs = {l["id"]: l for l in json.loads((ROOT / "data/world/locations.json").read_text(encoding="utf-8"))["locations"]}
os.chdir(tempfile.mkdtemp())
modes = [("vanilla", "anything"), ("shuffle", "shuffle"), ("anything", "hm_with_badge"), ("shuffle", "hm_with_badge")]
bad = 0
for seed in range(1, 9):
    players = []
    for k, (b, t) in enumerate(modes):
        players.append({"name": f"P{k}", "version": "black" if k % 2 else "white",
                        "options": {"shuffle_badge_rewards": b, "shuffle_tm_rewards": t}})
    cfg = {"schema": 1, "seed": seed, "options": {}, "players": players}
    open("rw.json", "w").write(json.dumps(cfg))
    r = subprocess.run([gen, "world", "rw.json", "rw-out.json"], capture_output=True, text=True)
    if r.returncode:
        print("seed", seed, "FAILED", r.stderr.strip())
        bad += 1
        continue
    w = json.load(open("rw-out.json"))
    for k, p in enumerate(w["players"]):
        b, t = modes[k]
        for loc, item, rcv in p["locations"]:
            l = locs.get(loc)
            if not l:
                continue
            it = items[item]
            own = rcv == p["slot"]
            if l["kind"] == "badge" and b != "anything":
                ok = own and it["group"] == "badge" and (b == "shuffle" or item == 0x1000 + ((loc - 0x10000) >> 4) - 370)
                if not ok:
                    print("seed", seed, p["name"], "badge rule broken at", l["name"], it["name"]); bad += 1
            if l["kind"] == "tm" and t != "anything":
                if not (own and it["group"] in ("tm", "hm")):
                    print("seed", seed, p["name"], "tm rule broken at", l["name"], it["name"]); bad += 1
            if t == "hm_with_badge" and own and (it["group"] == "hm" or it["name"].startswith("TM70")):
                if not (l["kind"] == "tm" and loc & 0xF == 8):
                    print("seed", seed, p["name"], "HM not at a badge TM:", it["name"], l["name"]); bad += 1
        # badges and HMs of a restricted player are nowhere else
    for k, p in enumerate(w["players"]):
        b, t = modes[k]
        for q in w["players"]:
            for loc, item, rcv in q["locations"]:
                if rcv != p["slot"]:
                    continue
                it = items[item]
                l = locs.get(loc)
                if b != "anything" and it["group"] == "badge" and not (q is p and l and l["kind"] == "badge"):
                    print("seed", seed, p["name"], "badge elsewhere", it["name"]); bad += 1
print("reward rules: 8 seeds x 4 players, problems:", bad)
sys.exit(1 if bad else 0)
