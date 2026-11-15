"""Convert PokemonBWAP's world data (MIT) into arkhipelagos' own data files.

usage (with a Python that has Archipelago's dependencies):
  python tools/bwap2data.py <BWAP apworld dir (pokemon_bw)> <Archipelago dir>

Writes data/world/*.json. BWAP is the reference; the files are ours and are
edited by hand from here on. Each file records where it came from.

What is converted:
- items: name, the game's item number, class (progression, useful, filler)
- regions, connections (with rules), story events (with rules)
- locations: item balls, hidden items, gifts and other NPC items, gym badges,
  gym TMs and TM/HM gifts (flag locations)
- rules: every named rule of BWAP's rules.py, translated into our rule
  language by reading its syntax tree. A construct the translator does not
  know becomes {"todo": "<source>"} and is listed at the end.

Permanent numbers (Archipelago item and location ids, fixed once released):
- item id = the game's item number (1 to 0x027F); badges 0x1000 + bit;
  seasons 0x1100 + index
- location id = 0x10000 + the event flag that marks it (flag locations)
These come from game facts, so they never depend on list order.
"""
import ast
import importlib.util
import inspect
import json
import os
import sys
import types

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT = os.path.join(ROOT, "data", "world")
SOURCE = "PokemonBWAP 0.4.5 data (BlastSlimey and contributors, MIT), converted by tools/bwap2data.py"

ITEM_BADGE_BASE = 0x1000
ITEM_SEASON_BASE = 0x1100
LOC_FLAG_BASE = 0x10000


def load_package(apw, ap):
    os.environ.setdefault("SKIP_REQUIREMENTS_UPDATE", "1")
    sys.path.insert(0, ap)
    spec = importlib.util.spec_from_file_location("pokemon_bw", os.path.join(apw, "__init__.py"),
                                                  submodule_search_locations=[apw])
    sys.modules["pokemon_bw"] = importlib.util.module_from_spec(spec)


# ---------------------------------------------------------------- rules

class RuleTranslator(ast.NodeVisitor):
    """lambda state, world: <expr>  ->  rule language (JSON)"""

    def __init__(self, src, names):
        self.src = src
        self.names = names   # names of other named rules
        self.todo = []

    def text(self, node):
        return ast.get_source_segment(self.src, node) or ast.dump(node)

    def bad(self, node):
        t = self.text(node)
        self.todo.append(t)
        return {"todo": t}

    def tr(self, n):
        if isinstance(n, ast.BoolOp):
            parts = [self.tr(v) for v in n.values]
            return {"all": parts} if isinstance(n.op, ast.And) else {"any": parts}
        if isinstance(n, ast.UnaryOp) and isinstance(n.op, ast.Not):
            return {"not": self.tr(n.operand)}
        if isinstance(n, ast.Constant) and isinstance(n.value, bool):
            return n.value
        if isinstance(n, ast.Call):
            return self.call(n)
        if isinstance(n, ast.Compare):
            return self.compare(n)
        if isinstance(n, ast.Attribute):
            # world.options.modify_logic.is_require_flash
            chain = self.attr_chain(n)
            if chain and chain[:2] == ["world", "options"] and len(chain) == 4 and chain[3].startswith("is_"):
                return {"option": chain[2], "has": chain[3][3:]}
            if chain and chain[:2] == ["world", "options"] and len(chain) == 3:
                return {"option": chain[2], "set": True}
        return self.bad(n)

    def attr_chain(self, n):
        out = []
        while isinstance(n, ast.Attribute):
            out.append(n.attr)
            n = n.value
        if isinstance(n, ast.Name):
            out.append(n.id)
            return list(reversed(out))
        return None

    def items(self, n):
        """a tuple/list of strings, or a known item group"""
        if isinstance(n, (ast.Tuple, ast.List)) and all(isinstance(e, ast.Constant) for e in n.elts):
            return [e.value for e in n.elts]
        chain = self.attr_chain(n)
        if chain and chain[0] == "tm_hm" and len(chain) == 2:
            return {"group": chain[1]}             # tm or hm
        if chain and chain[0] == "world" and len(chain) == 2:
            return {"world": chain[1]}             # a set computed at generation
        return None

    def call(self, n):
        f = n.func
        chain = self.attr_chain(f)
        if chain and chain[0] == "state":
            m = chain[1]
            a0 = n.args[0] if n.args else None
            if m == "has":
                if isinstance(a0, ast.Constant):
                    return {"has": a0.value}
                its = self.items(a0)
                if isinstance(its, dict) and "world" in its:
                    return {"has_value": its["world"]}
            if m in ("has_any", "has_all"):
                its = self.items(a0)
                key = "any" if m == "has_any" else "all"
                if isinstance(its, list):
                    return {key: [{"has": x} for x in its]}
                if isinstance(its, dict) and "group" in its:
                    return {"has_" + key + "_group": its["group"]}
                if isinstance(its, dict) and "world" in its:
                    return {"has_" + key + "_set": its["world"]}
            if m == "can_reach_region" and isinstance(a0, ast.Constant):
                return {"reach": a0.value}
        if isinstance(f, ast.Name) and f.id in self.names:
            return {"rule": f.id}
        if isinstance(f, ast.Name) and f.id == "build_seen_ext_rule" and isinstance(n.args[0], ast.Constant):
            return {"seen_count": n.args[0].value}
        return self.bad(n)

    def compare(self, n):
        if len(n.ops) == 1 and isinstance(n.comparators[0], ast.Constant):
            chain = self.attr_chain(n.left)
            v = n.comparators[0].value
            if chain and chain[:2] == ["world", "options"]:
                opt = chain[2] if len(chain) == 3 else ".".join(chain[2:])
                if isinstance(n.ops[0], ast.Eq):
                    return {"option": opt, "eq": v}
                if isinstance(n.ops[0], ast.NotEq):
                    return {"not": {"option": opt, "eq": v}}
        if len(n.ops) == 1 and isinstance(n.ops[0], ast.In) and isinstance(n.left, ast.Constant):
            chain = self.attr_chain(n.comparators[0])
            if chain and chain[:2] == ["world", "options"]:
                return {"option": chain[2], "has": n.left.value}
        return self.bad(n)


def translate_rules(path):
    src = open(path, encoding="utf-8").read()
    tree = ast.parse(src)
    names = set()
    lambdas = {}
    for node in tree.body:
        if isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.add(node.target.id)
            lambdas[node.target.id] = node.value
    tr = RuleTranslator(src, names)
    rules, include = {}, {}
    for name, val in lambdas.items():
        if isinstance(val, ast.Lambda):
            args = [a.arg for a in val.args.args]
            target = include if args == ["world"] else rules
            target[name] = tr.tr(val.body)
        elif isinstance(val, ast.Call):
            target = rules
            target[name] = tr.tr(val)
    return rules, include, tr.todo


# ---------------------------------------------------------------- objects

def rule_ref(obj, names, chain_code, todo):
    """A rule object (function, AND/OR/IF tuple, None) -> rule language."""
    if obj is None:
        return True
    cls = type(obj).__name__
    if cls in ("AndExtRules", "OrExtRules"):
        key = "all" if cls == "AndExtRules" else "any"
        return {key: [rule_ref(x, names, chain_code, todo) for x in obj]}
    if cls == "IfExtRules":
        return {"if": include_ref(obj[0], names, todo), "then": rule_ref(obj[1], names, chain_code, todo)}
    if callable(obj):
        if id(obj) in names:
            return {"rule": names[id(obj)]}
        if getattr(obj, "__code__", None) is chain_code:
            return {"has": "[Event] " + obj.__closure__[0].cell_contents}
    t = repr(obj)
    todo.append(t)
    return {"todo": t}


def include_ref(obj, names, todo):
    if obj is None:
        return True
    if callable(obj) and id(obj) in names:
        return {"include": names[id(obj)]}
    todo.append(repr(obj))
    return {"todo": repr(obj)}


def main():
    apw, ap = sys.argv[1], sys.argv[2]
    load_package(apw, ap)
    import pokemon_bw.data as D  # noqa: F401
    from pokemon_bw.data.locations import rules as R
    from pokemon_bw.data.locations import regions as REG, region_connections as RC, story_events as EV
    from pokemon_bw.data.locations.ingame_items import overworld_items, hidden_items, other, special
    from pokemon_bw.data.items import (badges, berries, key_items, main_items, medicine, other as other_items,
                                       seasons, tm_hm, classification as CL)
    from pokemon_bw.data.locations import progress_type as PT

    os.makedirs(OUT, exist_ok=True)
    todo = []
    rules, include, rtodo = translate_rules(inspect.getsourcefile(R))
    todo += [("rules.py", t) for t in rtodo]
    names = {id(v): k for k, v in vars(R).items() if callable(v) and not isinstance(v, type)}
    chain_code = EV.chain_event("x").__code__

    # Hand translations of what the syntax walk cannot read:
    # - door shuffle is not in v1: shuffled_doors is false, vanilla_doors true
    # - driftveil_random_tm: has the generated TM and the generated species
    # - monkeys_not_split: a value the generator computes from the statics
    include_out = {k: v for k, v in include.items()}
    include_out["shuffled_doors"] = False
    include_out["vanilla_doors"] = True
    include_out["monkeys_not_split"] = {"value": "monkeys_not_split"}
    rules["driftveil_random_tm"] = {"all": [{"has_value": "driftveil_random_tm"},
                                            {"has_value": "other_locations_species"}]}
    todo = [t for t in todo if not (isinstance(t, tuple) and ("driftveil_random_tm" in t[1] or "door_shuffle" in t[1]
                                                              or "Dreamyard Gift Encounter" in t[1]))]

    # items
    cls_names = {id(v): k for k, v in vars(CL).items() if callable(v)}
    pt_names = {id(v): k for k, v in vars(PT).items() if callable(v)}

    def cls_of(c):
        n = cls_names.get(id(c))
        return {"always_progression": "progression", "always_useful": "useful", "always_filler": "filler",
                "always_trap": "trap"}.get(n, n)

    items = []
    for group, mod, tables in (
            ("key", key_items, ("special", "progression", "vanilla", "useless")),
            ("main", main_items, ("mail", "fossils", "min_once", "filler", "unused")),
            ("berry", berries, ("standard", "niche")),
            ("medicine", medicine, ("important", "table")),
            ("other", other_items, ("special",)),
            ("tm", tm_hm, ("tm",)), ("hm", tm_hm, ("hm",))):
        for t in tables:
            for name, d in getattr(mod, t).items():
                items.append({"id": d.item_id, "name": name, "group": group, "table": t,
                              "class": cls_of(d.classification), "game_item": d.item_id})
    for name, d in badges.table.items():
        items.append({"id": ITEM_BADGE_BASE + d.bit, "name": name, "group": "badge", "table": "badges",
                      "class": cls_of(d.classification), "game_item": d.item_id, "badge_bit": d.bit})
    for i, (name, d) in enumerate(seasons.table.items()):
        items.append({"id": ITEM_SEASON_BASE + i, "name": name, "group": "season", "table": "seasons",
                      "class": cls_of(d.classification), "flag": d.flag_id, "var_value": d.var_value})
    ids = [x["id"] for x in items]
    dup_ids = sorted({i for i in ids if ids.count(i) > 1})

    # regions
    regions = []
    for name, d in REG.region_list.items():
        regions.append({"name": name, "type": d.type, "map": d.map_id, "encounters": d.has_encounters,
                        "full_map": d.full_map, "include": include_ref(d.inclusion_rule, names, todo) if d.inclusion_rule else True})

    # connections
    connections = []
    for c in RC.connections:
        dd = c._asdict()
        fwd = rule_ref(dd.get("rule"), names, chain_code, todo)
        back = dd.get("rule_2", False)
        connections.append({
            "from": dd["region_1"], "to": dd["region_2"], "type": dd["type"],
            "rule": fwd,
            # BWAP: rule_2 False means the way back has the same rule
            "rule_back": fwd if back is False else rule_ref(back, names, chain_code, todo),
            "one_way": bool(dd.get("one_way", False)),
            "include": include_ref(dd.get("inclusion_rule"), names, todo) if dd.get("inclusion_rule") else True,
        })

    # events
    events = [{"name": "[Event] " + e.name, "region": e.region,
               "include": include_ref(e.inclusion_rule, names, todo) if e.inclusion_rule else True,
               "rule": rule_ref(e.access_rule, names, chain_code, todo)} for e in EV.table]

    # locations
    locations = []
    for kind, mod, tables in (("item_ball", overworld_items, ("table", "abyssal_ruins", "seasonal")),
                              ("hidden", hidden_items, ("table", "seasonal")),
                              ("npc", other, ("table", "seasonal", "dust_clouds")),
                              ("badge", special, ("gym_badges",)),
                              ("tm", special, ("gym_tms", "tm_hm_ncps"))):
        for t in tables:
            for name, d in getattr(mod, t).items():
                # one flag can mark several gifts of one person ("... #1",
                # "... #2"); the number after "#" is the sub-index
                sub = int(name.rsplit("#", 1)[1]) if "#" in name and name.rsplit("#", 1)[1].isdigit() else 0
                if t == "gym_tms":
                    sub = 8   # the TM a leader gives shares the badge's flag
                locations.append({
                    "id": LOC_FLAG_BASE + (d.flag_id << 4) + sub, "name": name, "kind": kind, "table": t,
                    "flag": d.flag_id, "region": d.region,
                    "progress": pt_names.get(id(d.progress_type), repr(d.progress_type)),
                    "include": include_ref(d.inclusion_rule, names, todo) if d.inclusion_rule else True,
                    "rule": rule_ref(d.rule, names, chain_code, todo),
                })
    lids = [x["id"] for x in locations]
    dup_locs = sorted({i for i in lids if lids.count(i) > 1})

    def dump(name, payload):
        with open(os.path.join(OUT, name), "w", encoding="utf-8") as f:
            json.dump({"_source": SOURCE, **payload}, f, indent=1, ensure_ascii=False)

    dump("items.json", {"items": items})
    dump("regions.json", {"regions": regions})
    dump("connections.json", {"connections": connections})
    dump("events.json", {"events": events})
    dump("locations.json", {"locations": locations})
    dump("rules.json", {"rules": rules, "include": include_out})
    print(f"items {len(items)} (duplicate ids {dup_ids}), regions {len(regions)}, connections {len(connections)}, "
          f"events {len(events)}, locations {len(locations)} (duplicate ids {dup_locs}), rules {len(rules)}, "
          f"include rules {len(include_out)}")
    from collections import Counter
    flat = Counter((t[1] if isinstance(t, tuple) else t)[:120] for t in todo)
    print(f"to do by hand: {len(todo)} ({len(flat)} distinct)")
    for t, n in flat.most_common(40):
        print(f"   {n:4d}  {t}")
    dupes = [x for x in locations if x["id"] in set(dup_locs)]
    for x in dupes[:12]:
        print("   same flag:", x["flag"], x["name"], x["table"], x["include"])


if __name__ == "__main__":
    main()
