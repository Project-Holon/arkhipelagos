"""Badge and TM rewards hold what shuffle_badge_rewards and
shuffle_tm_rewards allow."""

from test.bases import WorldTestBase

from .. import GAME_BLACK, GAME_WHITE, LOCATION_BY_ID


class RewardRules:
    game: str
    badge: str
    tm: str

    def test_rewards_follow_the_rules(self) -> None:
        from Fill import distribute_items_restrictive
        distribute_items_restrictive(self.multiworld)
        p = self.world.player
        for loc in self.multiworld.get_locations(p):
            if loc.address is None:
                continue
            kind = LOCATION_BY_ID[loc.address]["kind"]
            it = loc.item
            group = self.world.item_name_groups
            if kind == "badge":
                self.assertEqual(it.player, p, loc.name)
                self.assertIn(it.name, group["badge"], loc.name)
                if self.badge == "vanilla":
                    self.assertEqual(it.code, 0x1000 + ((loc.address - 0x10000) >> 4) - 370)
            if kind == "tm":
                self.assertEqual(it.player, p, loc.name)
                self.assertTrue(it.name in group["tm"] or it.name in group["hm"], f"{loc.name}: {it.name}")
        if self.tm == "hm_with_badge":
            for loc in self.multiworld.get_locations(p):
                it = loc.item
                if it and it.player == p and (it.name in self.world.item_name_groups["hm"] or it.name.startswith("TM70")):
                    self.assertEqual(LOCATION_BY_ID[loc.address]["kind"], "tm", loc.name)
                    self.assertEqual(loc.address & 0xF, 8, loc.name)


class TestBadgeShuffleHmWithBadge(RewardRules, WorldTestBase):
    game = GAME_WHITE
    badge, tm = "shuffle", "hm_with_badge"
    options = {"shuffle_badge_rewards": "shuffle", "shuffle_tm_rewards": "hm_with_badge"}


class TestBadgeVanillaTmShuffle(RewardRules, WorldTestBase):
    game = GAME_BLACK
    badge, tm = "vanilla", "shuffle"
    options = {"shuffle_badge_rewards": "vanilla", "shuffle_tm_rewards": "shuffle"}
