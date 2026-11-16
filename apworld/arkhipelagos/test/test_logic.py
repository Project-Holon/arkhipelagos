"""The Python logic must open exactly the locations the generator opens.

For each test world, the same configuration goes to the generator's
`ap-reach` (the Rust sweep) and to this world's rules (an Archipelago
state with the same items and every event it can reach). The reachable
locations and the goal must be the same, for no items, every progression
item, and many random sets of them."""

import json
import os
import random
import subprocess
import tempfile

from BaseClasses import CollectionState
from test.bases import WorldTestBase

from ..pack import helper
from .. import GAME_BLACK, GAME_WHITE

SUBSETS = int(os.environ.get("ARKHIPELAGOS_SUBSETS", "40"))


class LogicMatches:
    game: str

    def rust_reach(self, items: list[int]) -> tuple[set[int], bool]:
        w = self.world
        with tempfile.TemporaryDirectory() as d:
            cfg = os.path.join(d, "config.json")
            lst = os.path.join(d, "items.json")
            with open(cfg, "w", encoding="utf-8") as f:
                json.dump(w.config, f)
            with open(lst, "w", encoding="utf-8") as f:
                json.dump(items, f)
            r = subprocess.run([helper(), "ap-reach", cfg, "0", lst], capture_output=True, check=True)
        v = json.loads(r.stdout)
        return set(v["locations"]), v["goal"]

    def python_reach(self, items: list[int]) -> tuple[set[int], bool]:
        w = self.world
        state = CollectionState(self.multiworld)
        for i in items:
            state.collect(w.create_item(w.item_id_to_name[i]), prevent_sweep=True)
        events = [loc for loc in self.multiworld.get_locations(w.player) if loc.address is None]
        while True:
            got = [loc for loc in events if loc not in state.locations_checked and loc.can_reach(state)]
            if not got:
                break
            for loc in got:
                state.locations_checked.add(loc)
                state.collect(loc.item, prevent_sweep=True, location=loc)
        reached = {
            loc.address
            for loc in self.multiworld.get_locations(w.player)
            if loc.address is not None and loc.can_reach(state)
        }
        return reached, state.has(w.pack["goal"], w.player)

    def compare(self, items: list[int], what: str) -> None:
        self.assertEqual(self.python_reach(items), self.rust_reach(items), what)

    def test_logic_matches_the_generator(self) -> None:
        prog = list(self.world.pack["pool"]["progression"])
        self.compare([], "no items")
        self.compare(prog, "every progression item")
        rng = random.Random(1234)
        for k in range(SUBSETS):
            sub = [i for i in prog if rng.random() < rng.random()]
            self.compare(sub, f"random set {k}")

    def test_every_location_is_in_the_static_data(self) -> None:
        names = {loc.name for loc in self.multiworld.get_locations(self.world.player) if loc.address is not None}
        self.assertTrue(names <= set(self.world.location_name_to_id))
        self.assertEqual(len(names), len(self.world.pack["locations"]))


class TestBlackVanilla(LogicMatches, WorldTestBase):
    game = GAME_BLACK


class TestWhiteVanilla(LogicMatches, WorldTestBase):
    game = GAME_WHITE
    options = {"goal": "champion", "season_control": "changeable",
               "modify_logic": ["require_flash", "require_dowsing"]}


class TestWhiteRandom(LogicMatches, WorldTestBase):
    """arkhipelagos' tests/configs/all-random.json, player 2."""
    game = GAME_WHITE
    options = {
        "season_control": "changeable",
        "species_sources": ["static", "evolutions", "form_change", "trainers", "trades"],
        "randomize_wild_pokemon": ["randomize", "type_themed_areas"],
        "randomize_static_pokemon": ["randomize", "similar_stats"],
        "randomize_gift_pokemon": ["randomize"],
        "randomize_legendary_pokemon": ["randomize", "keep_legendary"],
        "randomize_trade_pokemon": ["randomize_offer", "randomize_request"],
        "randomize_starter_pokemon": ["randomize", "type_variety"],
        "randomize_trainer_pokemon": ["randomize", "similar_stats", "shuffle_gym_types", "rivals_keep_starter"],
        "randomize_evolutions": ["randomize", "follow_type"],
        "replace_evo_methods": ["trade"],
        "randomize_tm_hm_compatibility": ["randomize", "follow_evolutions"],
        "randomize_types": ["randomize", "follow_evolutions"],
        "randomize_base_stats": ["randomize", "follow_evolutions"],
        "randomize_catch_rates": ["shuffle"],
        "randomize_gender_ratio": ["randomize"],
        "randomize_abilities": ["randomize"],
        "randomize_held_items": ["randomize"],
        "randomize_levelup_movesets": ["randomize", "match_types"],
        "randomize_move_data": ["power", "type", "accuracy", "pp"],
        "randomize_type_chart": ["shuffle"],
    }

    def test_slot_data_carries_the_tables(self) -> None:
        from Fill import distribute_items_restrictive
        distribute_items_restrictive(self.multiworld)
        slot = self.world.fill_slot_data()
        self.assertEqual(len(slot["locations"]), len(self.world.pack["locations"]))
        self.assertEqual(slot["version"], "white")
        self.assertTrue(slot["tables"])
        self.assertEqual(slot["config"]["players"][0]["options"]["randomize_types"], ["follow_evolutions", "randomize"])
        own = sum(1 for _, item, p in slot["locations"] if p == self.world.player and item)
        self.assertGreater(own, 0)
        text = json.dumps(slot)
        # it travels whole through metaxy's bridge (2 MB per message)
        self.assertLess(len(text), 2_000_000)
        for k in ("slot", "name", "version", "options", "locations", "left_out", "starters", "picks", "tables"):
            self.assertIn(k, slot)
        dump = os.environ.get("ARKHIPELAGOS_DUMP_SLOT")
        if dump:
            with open(dump, "w", encoding="utf-8") as f:
                f.write(text)
