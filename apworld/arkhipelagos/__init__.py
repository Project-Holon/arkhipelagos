"""arkhipelagos: Pokemon Black and White for eidos in Archipelago.

Two games come from this world, one per cartridge version:
"Pokemon Black (arkhipelagos)" and "Pokemon White (arkhipelagos)". There is
no patch file: the eidos mod gets everything it needs from slot_data,
through metaxy's Archipelago bridge.

Every item and location number, the item pool, the logic and the
randomized tables come from the arkhipelagos generator (arkh-gen): the
static file in data/ and one pack per player (pack.py). This module only
turns them into Archipelago regions, locations, items and rules."""

from typing import Any, ClassVar, Mapping

from BaseClasses import CollectionState, Entrance, Item, ItemClassification, Location, Region, Tutorial
from Options import OptionError
from worlds.AutoWorld import WebWorld, World

from .logic import Compiler, Effects, TOP_STEP, regions_read
from .options import ArkhOptions, generator_options
from .pack import STATIC, make_pack

GAME_BLACK = "Pokemon Black (arkhipelagos)"
GAME_WHITE = "Pokemon White (arkhipelagos)"
SLOT_FORMAT = "arkhipelagos-slot/1"

ITEM_NAME_TO_ID: dict[str, int] = {i["name"]: i["id"] for i in STATIC.get("items", [])}
ITEM_ID_TO_NAME: dict[int, str] = {v: k for k, v in ITEM_NAME_TO_ID.items()}
LOCATION_NAME_TO_ID: dict[str, int] = {loc["name"]: loc["id"] for loc in STATIC.get("locations", [])}
LOCATION_BY_ID: dict[int, dict[str, Any]] = {loc["id"]: loc for loc in STATIC.get("locations", [])}
ITEM_GROUPS: dict[str, set[str]] = {}
for _i in STATIC.get("items", []):
    if _i.get("group"):
        ITEM_GROUPS.setdefault(_i["group"], set()).add(_i["name"])


class ArkhItem(Item):
    pass


class ArkhLocation(Location):
    pass


class ArkhWeb(WebWorld):
    theme = "grass"
    tutorials = [Tutorial(
        "Setup guide",
        "How to play arkhipelagos in eidos with Archipelago.",
        "English",
        "setup_en.md",
        "setup/en",
        ["Project-Holon"],
    )]


class ArkhWorld(World):
    """Pokemon Black and White in eidos, the native PC port, with every item
    location shuffled across the multiworld and the arkhipelagos
    randomizers."""

    version_name: ClassVar[str] = ""
    options_dataclass = ArkhOptions
    options: ArkhOptions
    web = ArkhWeb()
    topology_present = True
    origin_region_name = "Menu"

    def generate_early(self) -> None:
        pool = [int(x) for x in self.options.starter_pool.value]
        if any(not 1 <= x <= 649 for x in pool):
            raise OptionError(f"{self.player_name}: starter_pool holds National Dex numbers, 1 to 649")
        self.config: dict[str, Any] = {
            "schema": 1,
            "seed": self.random.getrandbits(64),
            "options": {},
            "players": [{
                "name": self.player_name,
                "version": self.version_name,
                "options": generator_options(self.options),
            }],
        }
        try:
            self.pack: dict[str, Any] = make_pack(self.config)
        except RuntimeError as e:
            raise OptionError(f"{self.player_name}: {e}") from e
        self.origin_region_name = self.pack["menu"]
        p = self.pack["pool"]
        self.prog_ids = set(p["progression"])
        self.useful_ids = set(p["useful"])
        self.effects = Effects(self.pack)

    # ------------------------------------------------------------ regions

    def create_regions(self) -> None:
        pk = self.pack
        player = self.player
        comp = Compiler(player, pk["rules"])
        self.compiler = comp
        regions = {name: Region(name, player, self.multiworld) for name in pk["regions"]}
        self.multiworld.regions += regions.values()

        def entrance(a: str, b: str, rule: Any, k: int) -> None:
            if a not in regions or b not in regions or rule is False:
                return
            e = regions[a].connect(regions[b], f"{a} -> {b} ({k})")
            if rule is not True:
                e.access_rule = comp.rule(rule)
                for g in regions_read(rule, pk["rules"]):
                    if g in regions:
                        self.multiworld.register_indirect_condition(regions[g], e)

        for k, (a, b, one_way, rule, back) in enumerate(pk["connections"]):
            entrance(a, b, rule, k)
            if not one_way:
                entrance(b, a, back, k)

        for loc_id, rule in pk["locations"]:
            info = LOCATION_BY_ID[loc_id]
            region = regions[info["region"]]
            loc = ArkhLocation(player, info["name"], loc_id, region)
            if rule is not True:
                loc.access_rule = comp.rule(rule)
            region.locations.append(loc)

        def event(name: str, region_name: str, rule: Any, item: str) -> None:
            region = regions.get(region_name)
            if region is None or rule is False:
                return
            loc = ArkhLocation(player, name, None, region)
            if rule is not True:
                loc.access_rule = comp.rule(rule)
            loc.place_locked_item(ArkhItem(item, ItemClassification.progression, None, player))
            region.locations.append(loc)

        for k, (item, region_name, rule) in enumerate(pk["events"]):
            event(f"Event {k}: {item}", region_name, rule, item)

        # species: sources, evolutions, form changes, the Hall of Fame
        dex = pk["dex"]
        for k, (regs, rule, mons, seen, level) in enumerate(pk["sources"]):
            item = f"Species source {k}"
            fx = [("see", dex[m]) if seen else ("catch", m) for m in mons]
            fx.append(("level", level // 5))
            self.effects.add(item, fx)
            for g in regs:
                event(f"Species source {k} ({g})", g, rule, item)
        menu = self.pack["menu"]
        for k, (base, target, rule) in enumerate(pk["evolutions"]):
            item = f"Evolution {k}"
            self.effects.add(item, [("catch", target)])
            event(item, menu, ["all", ["caught", base], rule], item)
        for g, (region_name, mons, rules) in enumerate(pk["forms"]):
            any_caught = ["any"] + [["caught", m] for m in mons]
            for i, m in enumerate(mons):
                item = f"Form change {g}.{i}"
                self.effects.add(item, [("catch", m)])
                event(item, region_name, ["all", any_caught, rules[i]], item)
        if pk["hall"]:
            self.effects.add("Hall of Fame level", [("level", TOP_STEP)])
            event("Hall of Fame level", pk["hall"], True, "Hall of Fame level")

        goal = pk["goal"]
        self.multiworld.completion_condition[player] = lambda s: s.has(goal, player)

    # -------------------------------------------------------------- items

    def create_item(self, name: str) -> ArkhItem:
        code = ITEM_NAME_TO_ID[name]
        if code in self.prog_ids:
            c = ItemClassification.progression
        elif code in self.useful_ids:
            c = ItemClassification.useful
        else:
            c = ItemClassification.filler
        return ArkhItem(name, c, code, self.player)

    def create_items(self) -> None:
        n = len(self.pack["locations"])
        p = self.pack["pool"]
        prog = p["progression"]
        if len(prog) > n:
            raise OptionError(f"{self.player_name}: {len(prog)} progression items for {n} locations")
        ids = prog + p["useful"][: n - len(prog)]
        while len(ids) < n:
            ids.append(self.random.choice(p["filler"]))
        self.multiworld.itempool += [self.create_item(ITEM_ID_TO_NAME[i]) for i in ids]

    def get_filler_item_name(self) -> str:
        return ITEM_ID_TO_NAME[self.random.choice(self.pack["pool"]["filler"])]

    # ------------------------------------------------------------ species

    def collect(self, state: CollectionState, item: Item) -> bool:
        changed = super().collect(state, item)
        if changed:
            self.effects.apply(state, self.player, item.name, 1)
        return changed

    def remove(self, state: CollectionState, item: Item) -> bool:
        changed = super().remove(state, item)
        if changed:
            self.effects.apply(state, self.player, item.name, -1)
        return changed

    # ---------------------------------------------------------- slot data

    def fill_slot_data(self) -> Mapping[str, Any]:
        locations = []
        for loc in self.multiworld.get_locations(self.player):
            if loc.address is None or loc.item is None:
                continue
            it = loc.item
            own = it.code if (it.player == self.player and it.code is not None) else 0
            locations.append([loc.address, own, it.player])
        locations.sort()
        slot = dict(self.pack["slot"])
        slot.update({
            "format": SLOT_FORMAT,
            "generator": self.pack["generator"],
            "data_hash": self.pack["data_hash"],
            "player": self.player,
            "config": self.config,
            "locations": locations,
        })
        return slot


class ArkhBlackWorld(ArkhWorld):
    """Pokemon Black in eidos, with every item location shuffled across the
    multiworld and the arkhipelagos randomizers."""

    game = GAME_BLACK
    version_name = "black"
    item_name_to_id = ITEM_NAME_TO_ID
    location_name_to_id = LOCATION_NAME_TO_ID
    item_name_groups = ITEM_GROUPS


class ArkhWhiteWorld(ArkhWorld):
    """Pokemon White in eidos, with every item location shuffled across the
    multiworld and the arkhipelagos randomizers."""

    game = GAME_WHITE
    version_name = "white"
    item_name_to_id = ITEM_NAME_TO_ID
    location_name_to_id = LOCATION_NAME_TO_ID
    item_name_groups = ITEM_GROUPS
