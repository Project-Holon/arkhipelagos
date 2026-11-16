# The field-script tool

`crates/script` reads and writes the field scripts of Pokemon Black and
White (archive a/0/5/7) for all 12 cartridges. The mod uses it to put its
script changes into each player's own files at start; we use it to write
those changes once, as text.

## The command table

`data/script/commands.tsv` lists every command per language family:
opcode, name, arguments, whether it ends the flow, how often the
cartridges' scripts use it, a check against BWAP's table, and the source
file of its handler.

How it is made (`tools/`, Python, run on a machine with the cartridges and
aletheia):

1. `cmdtable.py` reads the VM's command table (function pointers) from each
   cartridge's ov010: 609 commands in the ten European cartridges, 605 in
   the two Japanese ones. European entries are named through aletheia's
   language maps, Japanese ones by the measured opcode rule (below).
2. `cmdargs.py` reads each handler in aletheia's C source with the
   language switches applied, and lists what it reads from the script, in
   order: `VM_ReadU16`, `VM_GetU32`, single bytes (`vm->pc++`), the work
   value and work pointer helpers, the helpers it calls with the VM, and
   the wait callbacks it sets. A 4-byte value added to the program counter
   is a target: code (passed to `VM_Jump` or `VM_Call`) or data (movement
   lists, warp lists).
3. `scriptdis.py check` decodes every script of the 12 cartridges with the
   table, following control flow. Result: 5,628 scripts, 0 errors, 96.9%
   of the bytes reached (the rest is dead code, padding and data nothing
   points at).
4. `exporttable.py` writes the TSV.

Japanese opcodes: the same as English below 0x4D; the word-buffer block
differs (English 0x4E and 0x4F have no Japanese entry; English 0x57/0x58
and 0x61/0x62 each become one Japanese command, at 0x55 and 0x5E; 0x4D,
0x55, 0x5E have Japanese-only bodies); from Japanese 0x5F on, English is
the Japanese opcode plus 4.

BWAP's table (MIT) was used only as a cross-check: it agrees on 570 of 599
European commands. Where it differs, aletheia's source decides, and the
row says `bwap-differs`.

## Formats

- Script member: an entry table (u32 offsets relative to their own end,
  ended by 0xFD13), then commands (u16 opcode and arguments), then data
  reached through relative offsets. 471 per cartridge (two of them empty).
- Zone member: the zone's script triggers (type 1: a list of work variable,
  value, script rows; types 2 to 4: a script to run on map load or
  recover). 427 per cartridge.
- One member (865, the hidden items) is a script member with no 0xFD13:
  its entry table ends where the first entry's code starts. The tool keeps
  it as it is; `tools/locsites.py` reads it.
- Script ids map to members through a table of 46 ranges in ov010
  (`SCRIPT_SYS_GetScriptFileId`): 2800-2819 member 862 (shared routines),
  3000-6999 member 863 (trainers), 7000-7799 member 864 (item balls),
  8000-8799 member 865 (hidden items); ids below 2000 are the zone's own
  member.

## Item sites (`tools/locsites.py`)

For each location, where the game gives its item, in all 12 cartridges
(written to `data/world/item_sites.json`):

| How | Count | Where |
|---|---|---|
| ball | 276 | map object with script 7000 + e; flag from the object (a/1/2/5); 864 `L23e2` |
| hidden | 146 | background event with script 8000 + e; 865 entry e calls 2811 (862 `L01ae`) |
| routine | 27 | NPC gift through 2800 (862 `L003e`), flag in work 0x8002 |
| give | 119 | a give through a flagless routine (2801, 2802, 2805, 2806, 2807, 2814) |
| trainer | 42 | the shared after-battle gift of 863 (`L0c01`); flag 1420 + trainer |
| badge | 8 | `SetBadge N` in the gym scripts; flag 370 + N |

Give sites: for the game's own flags, the give before the `SetEventFlag`;
for the flags PokemonBWAP added (370 to 480: its patched scripts set them,
the original game never does), the give is taken from BWAP's patched
script and matched to the original line by line (six by their order in
the member, all six checked by hand). Japanese command positions differ,
so each site has one per family, checked in all 12 cartridges.

14 hidden items have BWAP numbers (438 to 461) that are not the game's
flags; their sites carry `game_flag` (the game sets 954 to 1025), paired by
zone event file and order. 20 locations have no site (`none`, with the
reason: items chosen at run time, flags set nowhere, BWAP's own new items);
the generator leaves them out, so they keep the game's items.

## Check edits (`crates/apply/src/scripts.rs`)

The mod changes the give scripts so that they set the location's flag
with a marker instead of giving the item (the contract is in
`docs/v6-calls.md`). The edits are the same for every world:

- 862 `L003e` (NPC gifts with the flag in 0x8002), 862 `L01ae` (hidden
  items), 864 `L23e2` (item balls: the object's flag, then the object is
  removed), 863's after-battle gift (`SetTrainerDefeatedFlag` again, with
  the marker);
- every give site: the `LoadWork 0x8000 item` becomes a jump to new code
  that loads the original item and count into 0x8008 and 0x8009, the key
  into 0x805E, sets the flag with the marker, sets 0x8010 to 1 and jumps
  past the give;
- `SetBadge N`: the flag 370 + N with the marker instead (0x8008 is 0: the
  mod gives the badge itself when the location is not in the world); in
  the gym leaders' members, `CheckBadge N` becomes `CheckEventFlag 370 + N`
  (same size), so a beaten leader stays beaten whoever holds the badge.

`edit.rs` does the two edit kinds that never move bytes: a detour (the
covered commands become a jump padded with `Nop`, the new code is
appended) and a same-size replace. `arkh-apply scripts` checks each
edited member: every byte outside the edits is the original, and the
member disassembles and assembles back the same. `tools/check_apply.py`
then reads every edited member back on all 12 cartridges and compares the
checks it finds with item_sites.json: 78 members, 148 checks, 8,390 bytes
added, the same in every cartridge.

## Edits

The tool never moves existing bytes. It can:

- add scripts: the entry table grows, everything after it shifts by 4 per
  entry, and relative offsets stay valid;
- change commands in place (same size);
- append new code at the end, reached by a new entry or a jump.

## Text form

```
.entries L0006 L0012
L0006:
    SCRIPT_WORK_CmdLoadWorkConst 0x8020 0x0
    SCRIPT_WORK_CmdCall @L0123
.data movement 0100040000fe0000
.raw 0000
```

Commands are written by name, so one listing assembles for either family.

## Checks (`tools/check_scripts.py`)

For each of the 12 cartridges, every member of a/0/5/7:

- script: bytes to script to bytes, identical; bytes to text to bytes,
  identical; adding an entry and a new script leaves every existing item
  unchanged;
- zone: bytes to table to bytes, and through text, identical;
- the archive repacks byte for byte.

All 12 cartridges pass (3.5 seconds).

`arkh-script port` assembles one cartridge's scripts for another family
and compares with that cartridge's own members. English Black to
Japanese Black: 448 of 471 identical, 4 differ only in operands, 16 differ
in content, 3 use an English-only command. English Black to French Black:
467 identical. English Black to English White: 471 identical.

## Usage

```
cargo build --release
arkh-script check <eu|ja> <a/0/5/7>
arkh-script dis <eu|ja> <a/0/5/7> <member>
arkh-script asm <eu|ja> <listing> <out>
arkh-script port <eu|ja> <from a/0/5/7> <eu|ja> <to a/0/5/7>
```
