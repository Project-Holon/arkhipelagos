"""The generator's rules as Archipelago access rules.

A pack's rules are JSON (crates/gen/src/ap.rs): `true`, `false`,
`["has", item, n]`, `["all", ...]`, `["any", ...]`, `["not", r]`,
`["reach", region]`, `["rule", i]`, `["if", c, t]`, `["set", s]`,
`["seen", n]`, `["caught", mon]`, `["level", l]`. Options are already
folded in by the generator.

Species work as in the generator's sweep: each species source, evolution
and form change is an event; when the event is collected, `collect` below
counts the species caught and seen, the species sets hit and the levels
reached, in the player's state under names no item has."""

from typing import Any, Callable

from BaseClasses import CollectionState

Rule = Callable[[CollectionState], bool]

CAUGHT = "arkh caught "
SEEN_DEX = "arkh seen dex "
SEEN = "arkh seen"
SET = "arkh set "
LEVEL = "arkh level "
TOP_STEP = 20  # level 100 in steps of 5


def always(_: CollectionState) -> bool:
    return True


def never(_: CollectionState) -> bool:
    return False


class Compiler:
    def __init__(self, player: int, named: list[Any]):
        self.player = player
        self.named = named
        self.done: dict[int, Rule] = {}

    def rule(self, r: Any) -> Rule:
        p = self.player
        if r is True:
            return always
        if r is False:
            return never
        op = r[0]
        if op == "has":
            name, n = r[1], r[2]
            return lambda s: s.has(name, p, n)
        if op == "all":
            parts = [self.rule(x) for x in r[1:]]
            return lambda s: all(f(s) for f in parts)
        if op == "any":
            parts = [self.rule(x) for x in r[1:]]
            return lambda s: any(f(s) for f in parts)
        if op == "not":
            f = self.rule(r[1])
            return lambda s: not f(s)
        if op == "reach":
            region = r[1]
            return lambda s: s.can_reach_region(region, p)
        if op == "rule":
            i = r[1]
            if i not in self.done:
                # a named rule is compiled once; the cell breaks cycles
                cell: list[Rule] = []
                self.done[i] = lambda s: cell[0](s)
                cell.append(self.rule(self.named[i]))
                self.done[i] = cell[0]
            return self.done[i]
        if op == "if":
            c, t = self.rule(r[1]), self.rule(r[2])
            return lambda s: (not c(s)) or t(s)
        if op == "set":
            key = SET + str(r[1])
            return lambda s: s.has(key, p)
        if op == "seen":
            n = r[1]
            return lambda s: s.has(SEEN, p, n)
        if op == "caught":
            key = CAUGHT + str(r[1])
            return lambda s: s.has(key, p)
        if op == "level":
            if r[1] // 5 == 0:
                return always  # the generator starts at level 0
            keys = [LEVEL + str(k) for k in range(r[1] // 5, TOP_STEP + 1)]
            return lambda s: any(s.has(k, p) for k in keys)
        raise ValueError(f"unknown rule {r!r}")


def regions_read(r: Any, named: list[Any], seen: set[int] | None = None) -> set[str]:
    """The regions a rule asks to reach (through named rules too)."""
    seen = set() if seen is None else seen
    out: set[str] = set()
    if not isinstance(r, list):
        return out
    if r[0] == "reach":
        out.add(r[1])
    elif r[0] == "rule":
        if r[1] not in seen:
            seen.add(r[1])
            out |= regions_read(named[r[1]], named, seen)
    else:
        for x in r[1:]:
            out |= regions_read(x, named, seen)
    return out


class Effects:
    """What collecting a species event does to the player's counts."""

    def __init__(self, pack: dict[str, Any]):
        self.dex: list[int] = pack["dex"]
        self.sets_of: dict[int, list[int]] = {int(k): v for k, v in pack["sets_of"].items()}
        # event item name -> list of ("catch", mon) / ("see", dex) / ("level", step)
        self.by_item: dict[str, list[tuple[str, int]]] = {}

    def add(self, item: str, effects: list[tuple[str, int]]) -> None:
        self.by_item[item] = effects

    def apply(self, state: CollectionState, player: int, item: str, d: int) -> None:
        effects = self.by_item.get(item)
        if not effects:
            return
        c = state.prog_items[player]
        for kind, x in effects:
            if kind == "catch":
                key = CAUGHT + str(x)
                before = c[key]
                c[key] = before + d
                if (before == 0) != (c[key] == 0):
                    step = 1 if before == 0 else -1
                    self._see(c, self.dex[x], step)
                    for s in self.sets_of.get(x, ()):
                        c[SET + str(s)] += step
            elif kind == "see":
                self._see(c, x, d)
            else:
                c[LEVEL + str(x)] += d

    @staticmethod
    def _see(c: Any, dex: int, d: int) -> None:
        key = SEEN_DEX + str(dex)
        before = c[key]
        c[key] = before + d
        if before == 0 and c[key] > 0:
            c[SEEN] += 1
        elif before > 0 and c[key] == 0:
            c[SEEN] -= 1
