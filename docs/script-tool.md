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
- One member (865) is neither; it is kept as it is.

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
