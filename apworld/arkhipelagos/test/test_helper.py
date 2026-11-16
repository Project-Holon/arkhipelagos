"""The helper that cannot run stops generation with a clear message."""

import os
import unittest

from ..pack import make_pack, needs_helper


class HelperErrors(unittest.TestCase):
    def test_a_missing_program_says_what_to_do(self) -> None:
        old = os.environ.get("ARKHIPELAGOS_GEN")
        os.environ["ARKHIPELAGOS_GEN"] = os.path.join(os.path.dirname(__file__), "no-such-program")
        try:
            with self.assertRaises(RuntimeError) as e:
                make_pack({"schema": 1, "seed": 1, "options": {}, "players": []})
            self.assertIn("could not run the arkhipelagos generator", str(e.exception))
            self.assertIn("ARKHIPELAGOS_GEN", str(e.exception))
        finally:
            if old is None:
                del os.environ["ARKHIPELAGOS_GEN"]
            else:
                os.environ["ARKHIPELAGOS_GEN"] = old

    def test_which_options_need_the_program(self) -> None:
        base = {"goal": "ghetsis", "season_control": "vanilla", "modify_logic": ["require_flash"],
                "species_sources": ["static", "evolutions", "form_change", "trainers"], "starter_pool": [],
                "randomize_wild_pokemon": [], "replace_evo_methods": []}
        self.assertFalse(needs_helper(base))
        self.assertTrue(needs_helper({**base, "randomize_wild_pokemon": ["randomize"]}))
        self.assertTrue(needs_helper({**base, "replace_evo_methods": ["trade"]}))
        self.assertTrue(needs_helper({**base, "starter_pool": [1, 4, 7]}))
        self.assertTrue(needs_helper({**base, "species_sources": ["static"]}))
