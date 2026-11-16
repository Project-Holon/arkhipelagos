"""Player options. The names and values are the generator's own
(arkhipelagos docs/generator.md), so a player file means the same thing in
an Archipelago game and in an eidos-only game."""

import re
from dataclasses import dataclass

from Options import Choice, OptionList, OptionSet, PerGameCommonOptions


class Goal(Choice):
    """What finishes the game: defeating Ghetsis, or defeating the champion Alder."""
    display_name = "Goal"
    option_ghetsis = 0
    option_champion = 1
    default = 0


class SeasonControl(Choice):
    """vanilla: the season follows the calendar as in the game.
    changeable: the four season items are in the item pool, and the logic
    may ask for a season."""
    display_name = "Season control"
    option_vanilla = 0
    option_changeable = 1
    default = 0


class ModifyLogic(OptionSet):
    """require_flash: dark caves need Flash in logic.
    require_dowsing: hidden items need the Dowsing Machine in logic."""
    display_name = "Modify logic"
    valid_keys = ["require_flash", "require_dowsing"]


class SpeciesSources(OptionSet):
    """Where the logic finds species besides wild grass and water."""
    display_name = "Species sources"
    valid_keys = ["static", "evolutions", "form_change", "trainers", "trades"]
    default = frozenset({"static", "evolutions", "form_change", "trainers"})


class StarterPool(OptionList):
    """National Dex numbers (1 to 649) to draw the three starters from. Three
    or more turn it on; an empty list keeps the starters."""
    display_name = "Starter pool"
    default = []


def modifiers(name: str, doc: str, keys: list[str]) -> type[OptionSet]:
    """An option that is a list of modifiers; an empty list (the default)
    leaves that part of the game as it is."""
    display = re.sub(r"(?<!^)(?=[A-Z])", " ", name).capitalize().replace("pokemon", "Pokemon")
    # a module-level class of this name, so options pickle
    return type(name, (OptionSet,), {"__doc__": doc, "display_name": display, "valid_keys": keys,
                                      "__module__": __name__, "__qualname__": name})


RandomizeWildPokemon = modifiers(
    "RandomizeWildPokemon",
    """randomize: wild Pokemon are randomized. ensure_all: every species is
    found at least once. similar_stats: base stat totals stay close.
    type_themed_areas: each area keeps one type.""",
    ["randomize", "ensure_all", "similar_stats", "type_themed_areas"])
RandomizeStaticPokemon = modifiers(
    "RandomizeStaticPokemon",
    """randomize: static encounters are randomized. similar_stats, no_legendaries.""",
    ["randomize", "similar_stats", "no_legendaries"])
RandomizeGiftPokemon = modifiers(
    "RandomizeGiftPokemon",
    """randomize: gift Pokemon are randomized. similar_stats, no_legendaries,
    split_monkeys (else the three Dreamyard monkeys stay one species).""",
    ["randomize", "similar_stats", "no_legendaries", "split_monkeys"])
RandomizeLegendaryPokemon = modifiers(
    "RandomizeLegendaryPokemon",
    """randomize: legendary encounters are randomized. keep_legendary: a
    legendary for a legendary.""",
    ["randomize", "keep_legendary"])
RandomizeTradePokemon = modifiers(
    "RandomizeTradePokemon",
    """randomize_offer, randomize_request: what in-game trades give and ask for.""",
    ["randomize_offer", "randomize_request"])
RandomizeStarterPokemon = modifiers(
    "RandomizeStarterPokemon",
    """randomize: the starters are randomized. any_base (else first stages
    that evolve twice), similar_stats, type_variety.""",
    ["randomize", "any_base", "similar_stats", "type_variety"])
RandomizeTrainerPokemon = modifiers(
    "RandomizeTrainerPokemon",
    """randomize: trainer Pokemon are randomized. similar_stats, type_themed,
    shuffle_gym_types, rivals_keep_starter.""",
    ["randomize", "similar_stats", "type_themed", "shuffle_gym_types", "rivals_keep_starter"])
RandomizeEvolutions = modifiers(
    "RandomizeEvolutions",
    """randomize: new evolution targets (methods kept). follow_type: the
    target shares a type.""",
    ["randomize", "follow_type"])
ReplaceEvoMethods = modifiers(
    "ReplaceEvoMethods",
    """trade: trade evolutions become level, item or party evolutions.""",
    ["trade"])
RandomizeTmHmCompatibility = modifiers(
    "RandomizeTmHmCompatibility",
    """randomize: which species learn which machines. all_hms, all_tms,
    follow_evolutions.""",
    ["randomize", "all_hms", "all_tms", "follow_evolutions"])
RandomizeTypes = modifiers(
    "RandomizeTypes",
    """shuffle: one permutation of the types for all species. randomize:
    new types (dual stays dual). follow_evolutions.""",
    ["shuffle", "randomize", "follow_evolutions"])
RandomizeBaseStats = modifiers(
    "RandomizeBaseStats",
    """randomize: the six stats are permuted. follow_evolutions, random_total.""",
    ["randomize", "follow_evolutions", "random_total"])
RandomizeCatchRates = modifiers(
    "RandomizeCatchRates",
    """shuffle or randomize (3 to 255).""",
    ["shuffle", "randomize"])
RandomizeGenderRatio = modifiers(
    "RandomizeGenderRatio", """randomize: gender ratios (fixed ratios stay).""", ["randomize"])
RandomizeAbilities = modifiers(
    "RandomizeAbilities", """randomize: abilities (Wonder Guard stays).""", ["randomize"])
RandomizeHeldItems = modifiers(
    "RandomizeHeldItems", """randomize: wild held items shuffled between species.""", ["randomize"])
RandomizeLevelupMovesets = modifiers(
    "RandomizeLevelupMovesets",
    """randomize: level-up moves (levels and count kept). match_types.""",
    ["randomize", "match_types"])
RandomizeMoveData = modifiers(
    "RandomizeMoveData",
    """power, type, accuracy, pp: which move data change (machine moves keep theirs).""",
    ["power", "type", "accuracy", "pp"])
RandomizeTypeChart = modifiers(
    "RandomizeTypeChart", """shuffle: the type chart's cells are shuffled.""", ["shuffle"])


@dataclass
class ArkhOptions(PerGameCommonOptions):
    goal: Goal
    season_control: SeasonControl
    modify_logic: ModifyLogic
    species_sources: SpeciesSources
    starter_pool: StarterPool
    randomize_wild_pokemon: RandomizeWildPokemon
    randomize_static_pokemon: RandomizeStaticPokemon
    randomize_gift_pokemon: RandomizeGiftPokemon
    randomize_legendary_pokemon: RandomizeLegendaryPokemon
    randomize_trade_pokemon: RandomizeTradePokemon
    randomize_starter_pokemon: RandomizeStarterPokemon
    randomize_trainer_pokemon: RandomizeTrainerPokemon
    randomize_evolutions: RandomizeEvolutions
    replace_evo_methods: ReplaceEvoMethods
    randomize_tm_hm_compatibility: RandomizeTmHmCompatibility
    randomize_types: RandomizeTypes
    randomize_base_stats: RandomizeBaseStats
    randomize_catch_rates: RandomizeCatchRates
    randomize_gender_ratio: RandomizeGenderRatio
    randomize_abilities: RandomizeAbilities
    randomize_held_items: RandomizeHeldItems
    randomize_levelup_movesets: RandomizeLevelupMovesets
    randomize_move_data: RandomizeMoveData
    randomize_type_chart: RandomizeTypeChart


# option field name -> how it goes into the generator's configuration
CHOICES = ["goal", "season_control"]
SETS = [f for f in ArkhOptions.__annotations__ if f not in CHOICES + ["starter_pool"]
        and f not in PerGameCommonOptions.__annotations__]


def generator_options(o: ArkhOptions) -> dict:
    """The player's options as the generator reads them."""
    out: dict = {}
    for f in CHOICES:
        out[f] = getattr(o, f).current_key
    for f in SETS:
        out[f] = sorted(getattr(o, f).value)
    out["starter_pool"] = [int(x) for x in o.starter_pool.value]
    return out
