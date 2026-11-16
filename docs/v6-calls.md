# The eidos API calls the arkhipelagos mod uses

The mod is a wasm code mod (`crates/mod`, built by `tools/build_mod.sh`).
This list is exact: the mod imports these and nothing else from module
`eidos`. Item numbers point to `docs/eidos-api-proposal.md` (approved,
interface version 6). The wasm import names follow eidos's generator rule
(the C member name, `eidos_` prefix in C).

## Version 6 calls (new)

| Import | Proposal | What the mod does with it |
|---|---|---|
| `vfs_member_get` | item 1 | reads the player's own members to build its changes on (a/0/5/7 scripts; the table archives the apply step writes) |
| `vfs_member_set` | item 1 | puts the changed members in place at start: scripts (a/0/5/7), personal, learnsets, evolutions, moves, encounters, trainer parties |
| `vfs_member_clear` | item 1 | index 0xFFFFFFFF when the player leaves a world (back to the plain game) |
| `text_get` | item 2 | reads game lines to check its own (item and species names come from the cartridge) |
| `text_set` | item 2 | its own lines: "you found X for Y", "you received X from Y", the per-world picks' lines |
| `text_language` | item 2 | picks the line table for the cartridge's language |
| `work_get` | item 3 | inside the `flag.set` handler: work 0x805F (marker), 0x805E (gift number), 0x8008 and 0x8009 (the original item and count) |
| `work_set` | item 3 | clears the marker after a handled give (also done by the script itself) |
| `badge_set` | item 4 | gives a badge the player received as an item |
| `script_run` | item 5 | shows the mod's message script after a check or a received item |
| event `script.done` | item 5 | the message has ended: the next queued message may start |
| event `starter.choices` | item 12 | writes the world's three starters into the starter screen |
| `type_chart_set` | item 11 | writes the world's type chart when the type chart randomizer is on |
| `mod_file_write`, `mod_file_read`, `mod_file_list`, `mod_file_delete` | item 15 | stored solo worlds (configuration, seed, generator version, world hash) |
| `text_input` and event `text_input.done` | item 14 | the world code and the player name when joining online |
| `world_create`, `world_owner`, `world_join`, `world_checks`, `world_leave`, `on_world` | item 16 | online worlds on metaxy |
| `ap_link`, `ap_send`, `on_ap` | item 16 | Archipelago rooms (mode b, later) |
| `peer_position`, `townmap_marker_set`, `townmap_marker_clear` | item 17 | other players of the world (online, later) |

The solo flow (create, generate, apply, play) needs only the first block:
`vfs_member_get`, `vfs_member_set`, `vfs_member_clear`, `text_get`,
`text_set`, `text_language`, `work_get`, `work_set`, `badge_set`,
`script_run`, `script.done`, `starter.choices`, `type_chart_set` and the
four `mod_file_*` calls.

## Version 5 calls it relies on (already in eidos)

| Import or event | What for |
|---|---|
| `log` | the mod log |
| `on` | binds the handlers below |
| event `flag.set` | the check: bound at `ZONEDATA_SetEventFlag` 0x02014314, before the flag is set, so the handler runs while the giving script still runs and `work_get` sees its work values |
| event `game.new`, `save.load`, `save.begin` | start of a game; the mod's state with the save |
| `flag`, `flag_set` | reads and sets event flags (received key items that open the story) |
| `item_add`, `item_count` | gives items |
| `zone` | where the player is (messages, positions) |
| `state_load`, `state_store` | the mod's per-save state: checked locations, index of received items |

## How a check works (the contract with the scripts)

The mod's script edits (`data/script/edits`, applied at start) change the
shared give routines and some give sites so that, instead of giving the
original item, the script:

1. loads the original item and count into work 0x8008 and 0x8009;
2. loads the gift number n into work 0x805E (0 when a flag has one item);
3. loads the marker 0xA4C7 into work 0x805F;
4. sets the location's event flag (`SetEventFlag`);
5. clears work 0x805F.

The mod's `flag.set` handler: when `work_get(0x805F)` is 0xA4C7, the flag
and `work_get(0x805E)` give the location id `0x10000 + (flag << 4) + n`.
If that location is in the world, the mod sends the check and gives this
player's item (if any); if it is not, it gives the original item from
0x8008 and 0x8009. Then it queues its message (`script_run`). A flag set
without the marker is the game's own and is ignored.

Work 0x805E and 0x805F are script-local works that no original script
uses (all 899 members of all 12 cartridges checked).
