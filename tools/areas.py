"""Write data/world/areas.json: the Tracker's areas, one per place the
player knows (a town, a route, a cave), each with the locations whose names
start with it and the zones it covers.

    python tools/areas.py [--check]

An area's name is a location name's part before " - ", with the floors and
parts of one place joined (MERGE). Its zones, from the English cartridges
(Black and White, tools/carts.local.json):

- the zones whose event file holds one of its item balls (item_sites.json);
- the zones that are their own parent and whose place name (a/0/0/2 file
  89) is the area's name.

A zone header is 48 bytes in a/0/1/2 file 0: the event file at 0x16, the
parent zone at 0x18, the place name in the low 10 bits of 0x1A. eidos opens
the Tracker on the area listing the player's zone or its parent, so a
building's zone needs no line of its own. --check compares the file with
what the cartridges give.
"""
import json
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import carts  # noqa: E402
from textcodec import decode, narc_files, plain  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "data", "world", "areas.json")
PLACE_NAMES = 89
MAX_ZONES = 8  # eidos's Tracker keeps 8 zones per area

# parts of one place, by the start of their name
MERGE = [
    ("Abyssal Ruins", "Abyssal Ruins"),
    ("Victory Road", "Victory Road"),
    ("Giant Chasm", "Giant Chasm"),
    ("Dreamyard", "Dreamyard"),
    ("Pinwheel Forest", "Pinwheel Forest"),
    ("Cold Storage", "Cold Storage"),
    ("Route 3 or Dreamyard", "Route 3"),
]


def area_of(location_name):
    head = location_name.split(" - ")[0]
    for start, name in MERGE:
        if head.startswith(start):
            return name
    return head


def zones_of(code):
    data = carts.load()[code]["data"]
    raw = narc_files(os.path.join(data, "a", "0", "1", "2"))[0]
    names = [plain(t) for t in decode(narc_files(os.path.join(data, "a", "0", "0", "2"))[PLACE_NAMES])[0]]
    out = []
    for z in range(len(raw) // 48):
        (script,) = struct.unpack_from("<H", raw, z * 48 + 0x06)
        event, parent, place = struct.unpack_from("<HHH", raw, z * 48 + 0x16)
        place &= 0x3FF
        out.append({"zone": z, "script": script, "event": event, "parent": parent,
                    "place": names[place] if place < len(names) else ""})
    return out


def build():
    locs = json.load(open(os.path.join(ROOT, "data", "world", "locations.json"), encoding="utf-8"))["locations"]
    sites = json.load(open(os.path.join(ROOT, "data", "world", "item_sites.json"), encoding="utf-8"))["sites"]
    areas = {}
    for loc in locs:
        a = areas.setdefault(area_of(loc["name"]), {"events": set(), "scripts": set(), "zones": []})
        s = sites.get(str(loc["id"]))
        if s and "event_file" in s:
            a["events"].add(s["event_file"])
        for site in (s or {}).get("sites", []):
            if "member" in site:
                a["scripts"].add(site["member"])
    for code in ("IRBO", "IRAO"):
        zs = zones_of(code)
        # the place itself first, then the zones of its item balls; then,
        # for zones no area has yet, the zones whose scripts give its items
        # (a gym, a house); a zone whose parent is listed already is left
        # out (eidos finds it through the parent)
        def add(a, cands):
            for z in cands:
                if z["zone"] not in a["zones"] and z["parent"] not in a["zones"]:
                    a["zones"].append(z["zone"])
        for name, a in areas.items():
            add(a, [z for z in zs if z["place"] == name and z["parent"] in (z["zone"], 0)])
            add(a, [z for z in zs if z["event"] in a["events"]])
        claimed = {z for a in areas.values() for z in a["zones"]}
        for name, a in areas.items():
            add(a, [z for z in zs if z["script"] in a["scripts"] and z["zone"] not in claimed])
    out = []
    for name in sorted(areas, key=lambda n: (not n.startswith("Route "), int(n.split()[1]) if n.startswith("Route ") else 0, n)):
        zones = sorted(areas[name]["zones"][:MAX_ZONES])
        out.append({"name": name, "zones": zones})
    return {
        "_source": "tools/areas.py: location names (locations.json), item ball event files (item_sites.json), "
                   "and the zone headers and place names of the English Black and White cartridges",
        "merge": [list(m) for m in MERGE],
        "areas": out,
    }


def main():
    d = build()
    text = json.dumps(d, indent=1, ensure_ascii=False) + "\n"
    if "--check" in sys.argv:
        same = open(OUT, encoding="utf-8").read() == text
        print("areas.json matches the cartridges" if same else "areas.json differs from the cartridges")
        sys.exit(0 if same else 1)
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    empty = [a["name"] for a in d["areas"] if not a["zones"]]
    print(f"{len(d['areas'])} areas, {len(empty)} without a zone: {empty}")


if __name__ == "__main__":
    main()
