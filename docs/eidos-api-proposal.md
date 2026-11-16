# Proposal: eidos mod API additions for arkhipelagos

Approved on 2026-11-16, with three notes that are now written in: the
type chart (item 11), archives that are already open (item 1) and a size
quota for mod files (item 15). The eidos work itself is done in the eidos
repository; this document is the agreed design.

Every addition is general mod API (interface version 6), usable by any
mod, not only arkhipelagos. Where possible an addition is an **event with
a writable field**, so rule mods get it too (`on <event> do set <field>`);
services cover what an event cannot. As eidos's own rule says
(`docs/mod-backlog.md`), each binding is checked against the game's code
before it is built; the bindings below are named from aletheia's source
(English Black addresses; eidos binds by name, so the other cartridges
follow).

Two items are already on eidos's own backlog and are shaped here as it
describes them: "Type matchups: a writable type effectiveness in a
battle", and "Encounter tables: reading a zone's encounter slots".

## Summary

| # | Name | Kind | What a mod gets |
|---|---|---|---|
| 1 | `vfs_member_set` and friends | service | change a NARC member in memory, without files |
| 2 | `text_get`, `text_set` | service | read and set a text entry by archive, file, entry, block |
| 3 | `work_get`, `work_set` | state | script work variables |
| 4 | `badge_set` | state | give a badge |
| 5 | `script_run` | service | start a field script by number when the field is idle |
| 6 | `move.forget` | event, writable `allowed` | let HMs (or any move) be forgotten, or not |
| 7 | `mon.shiny` | event, writable `threshold` | the shiny chance |
| 8 | `item.usable` | event, writable `usable` | allow or refuse a field item (Gracidea first) |
| 9 | `season.change` | event, writable `season` | season control |
| 10 | `encounter.slot` | event, writable `slot` | which wild slot is picked |
| 11 | `type_chart_set`, `battle.type_effect` | service; event, writable `effect` | the type chart: written once (main path), or per lookup (rule mods) |
| 12 | `starter.choices` | event, writable `species[3]` | the three starters shown |
| 13 | `egg.species` | event, writable `species`, `rules` | what hatches, and the special cases |
| 14 | `text_input` | service | ask the player for text, on every device |
| 15 | `mod_file_*` | service | a mod's own files in the eidos documents folder, 16 MiB per mod |
| 16 | `world_*`, `ap_*` | services | metaxy world sessions and the Archipelago bridge |
| 17 | `peer_position`, `townmap_marker_*` | service | see other players in the field and on the Town Map |

## 1. Runtime member overrides

```c
int  (*vfs_member_set)(const char *archive, u32 index, const void *data, u32 size);
int  (*vfs_member_clear)(const char *archive, u32 index);   /* index 0xFFFFFFFF: all of the mod's */
int  (*vfs_member_get)(const char *archive, u32 index, void *out, u32 size); /* the member as the game would read it now */
```

- A layer above the mod roots, one per mod, in memory. It joins the
  existing merge in `sdk/vfs_narc.c`: the latest layer wins, so a mod's
  memory layer is placed where its folder would be in the load order.
- A change drops the cached merged archive for that path
  (`bw_vfs_flush` for one path), so the next open sees it.
- **Archives that are already open** (approval note). Some archives stay
  open from boot or for a whole scene (scripts, text, the Pokedex's 35
  message banks), so "set it before the game opens it" is not enough. The
  port also reaches the open ones:
  - The game reads an open archive through its `ArcHandle`, which keeps
    the member count, the FAT offset and the image base from the open
    (`ARC_TOOL_ReadHandleHeader`), and reads a member's FAT entry again on
    every load (`ARC_TOOL_ReadHandleMember` 0x0204932C: seek to
    `fatOffset + 0xC + index * 8`, then to `imageBase + 8 + start`).
  - So for an open archive the port builds a **layout-stable image**: the
    same header, file name table, member count and image base; changed
    members are written at the end of the image, and only their FAT
    entries change. Every offset the handle keeps stays true.
  - The port swaps that image into every open file of the archive
    (`BwFsSlot.mem` and the window size in `sdk/fs.c`; a slot that read
    the host file becomes a memory slot). The old image is freed when its
    last reader closes, as `stale` entries are today.
  - The swap is queued and done between frames, on the game thread, so no
    read is ever half on the old image and half on the new one.
  - A member past the end cannot be added to an open archive (the handle
    keeps the count): `vfs_member_set` returns -1 with a message. Growing
    an archive works when it is closed.
  - What the game already copied into its own memory (a script loaded by
    the VM, a decoded text bank) does not change until the game loads it
    again; a mod changes zone scripts and text before the map change or
    scene that loads them.
- `vfs_member_get` lets a mod build its change from the player's own
  member (arkhipelagos assembles its scripts against the player's own
  scripts).
- Limits (decided): **16 MiB per mod**, and 32 MiB for all mods'
  memory layers together; per member, the NARC format's own limit. Wasm
  mods pass a pointer into their memory; the port copies.
  - Why 16 MiB: the largest real case measured is a full randomizer with
    every option on (BWAP's all-randomizer seed ported to member files):
    4,202 members, 6.2 MB. arkhipelagos adds its assembled scripts and
    text (well under 1 MB). 16 MiB is about 2.5 times that.
  - Why it is safe on consoles and phones: the layer is plain memory held
    once; a merged archive is built only while that archive is open, as
    today. 16 MiB (32 MiB for all mods) is a small share of the Vita's
    512 MB and of a low-end phone's 2 to 3 GB, and it stays in the
    32-bit build's heap, not in its program image (the image limit below
    0x02000000 is about static data, which this is not).
  - A mod that needs more gets -1 and a message, never a crash.
- Other uses: any data mod that is computed, not shipped (randomizers,
  difficulty mods, translations).

## 2. Text entries

```c
int (*text_get)(u32 archive, u32 file, u32 entry, u32 block, u16 *out, u32 max_units);
int (*text_set)(u32 archive, u32 file, u32 entry, u32 block, const u16 *units, u32 count);
u32 (*text_language)(void);   /* the cartridge's language */
```

- `archive` is 2 (system text, a/0/0/2) or 3 (story text, a/0/0/3).
  Units are the game's 16-bit code units, control codes included.
  Japanese files have two blocks (kana and kanji); others one.
- Built on item 1 and the port's text decoding; setting an entry past the
  file's end grows the file (new lines for new scripts).

## 3. Work variables (state)

```c
int (*work_get)(u16 id, u16 *out);
int (*work_set)(u16 id, u16 value);
```

- Saved variables (0x4000 to 0x40FF) through `ZONEDATA_GetEventWorkAddress`
  0x020143CC; script-local ones (0x8000 and up) through
  `SCRIPT_WORK_GetWorkPtr` 0x02158FE0, only while a script runs (else -1).
- Rules get the same as a state field.

## 4. Give a badge (state)

```c
int (*badge_set)(u32 bit);
```

- Calls `MISC_SetBadge` 0x0200C248, where eidos already binds `badge.get`,
  so the event fires as for a badge won in a gym.

## 5. Start a field script

```c
int (*script_run)(u16 script_id, u32 *ticket);   /* queued */
```

- Runs when the field is idle (no script, no menu, no battle, no fade),
  through `SCRIPT_SYS_CreateScriptEventSimple` 0x02158768, the way the game
  starts a script from the field.
- A new event `script.done` (ticket) tells the mod it ended.
- Uses: the Archipelago menu, "you received X from Y" messages, any mod
  that adds scripted scenes.

## 6. `move.forget`

- Binding: `FILE_UTIL_IsHmMove` 0x020212E4 (the HM check of the
  forget-move screens).
- Payload: species, move, `allowed` (writable; the game's answer is "not
  an HM").
- Refuse: not refusable; `allowed` decides.

## 7. `mon.shiny`

- Binding: `POKE_TOOL_IsShinyOtIdPersonality` 0x02017ED8.
- Payload: trainer id, personality, `value` (the xor the game computes),
  `threshold` (writable, 8 in the game). Shiny when value < threshold.
- The same Pokemon gives the same answer every time, so display, eggs and
  battles agree (a random roll per call would not).

## 8. `item.usable`

- Binding: `PLIST_DEMO_CanUseGracidea` 0x021BDCC0 first; other "can this
  item be used here" checks of the party screen join later.
- Payload: item, party slot, `usable` (writable).

## 9. `season.change`

- Binding: where the game sets the season from the clock on a new day,
  `COMM_PLAYER_SUPPORT_GetSeasonFromClock` 0x0202E888 with
  `COMM_PLAYER_SUPPORT_SetSeason` 0x0202E8CC; the read side is
  `GAME_DATA_GetSeason` 0x02012984.
- Payload: the season the clock gives, `season` (writable).
- To be read before building: the overworld refresh after a change
  (BWAP's season patch touches ov20).

## 10. `encounter.slot`

- Binding: `FIELD_ENCOUNT_PickGroundSlot` 0x021A9DE0,
  `FIELD_ENCOUNT_PickWaterSlot` 0x021A9E64,
  `FIELD_ENCOUNT_PickFishingSlot` 0x021A9E98.
- Payload: method (ground, water, fishing), the roll, `slot` (writable).
- Also from the backlog: a state call to read a zone's slot table.

## 11. `type_chart_set` and `battle.type_effect`

```c
int (*type_chart_set)(const u8 chart[17][17]);   /* the game's codes: 0, 2, 4, 8 */
int (*type_chart_get)(u8 out[17][17]);
```

- **Main path (approval note): `type_chart_set`.** The chart is
  `gBtlCalcTypeAffinityTable` 0x021F047C, `u8 [17][17]`, in the battle
  overlay ov093. The game's codes are 0 (no effect), 2 (half), 4 (normal)
  and 8 (double). The port keeps the mod's chart and writes it over the
  table each time ov093 is loaded, so there is no cost per damage roll.
  The write also reaches code that reads the table directly, not through
  the lookup (`BTL_CALC_ListResistingTypes`, used by the AI).
- With several mods, the last one in the load order wins, as for members.
  `type_chart_get` returns the chart as it is now (the game's, or a mod's).
- **For rule mods: the event `battle.type_effect`.**
  Binding: `BTL_CALC_GetTypeAffinity` 0x021D794C. Payload: attacking type,
  defending type, `effect` (writable, the affinity class). It runs once
  per lookup, so once per damage roll and more in AI scoring; it costs
  nothing when no mod listens, and a rule mod uses it for conditional
  changes (one battle, one trainer). It sees the chart after
  `type_chart_set`.
- Together they are the backlog's "writable type effectiveness".

## 12. `starter.choices`

- Binding: the starter screen's set-up (`PSEL_Init` 0x021B9608, which reads
  `sPselIds`).
- Payload: `species[3]` (writable).
- The species actually given is set by the script (the mod's own script
  change), so the screen and the gift agree.

## 13. `egg.species`

- Binding: `SODATEYA_BuildEgg` 0x021C38F4.
- Payload: mother species, father species, `species` (writable), `rules`
  (writable bits: Nidoran, Volbeat and Illumise, Manaphy, baby incense,
  Shellos, Burmy, Basculin special cases).

## 14. `text_input`

```c
int (*text_input)(const u16 *title, u32 max_units, u32 kind, u32 *ticket);
```

- Opens the game's own name-entry screen (`NAMEIN_SETUP_CreateArgs`
  0x0216BFD4) with a title; works with a pad, touch, and a keyboard on PC.
- `kind`: name (the game's letters), code (letters and digits), link
  (all printable characters: for room links).
- The answer comes back in a `text_input.done` event (ticket, units).

## 15. Mod files

```c
int (*mod_file_write)(const char *name, const void *data, u32 size);
int (*mod_file_read)(const char *name, void *out, u32 size, u32 *got);
int (*mod_file_list)(char *out, u32 size);
int (*mod_file_delete)(const char *name);
```

- A folder per mod in the eidos documents folder (where saves live on
  each platform). Names are plain (no paths).
- The mod's saved data (`state_store`) stays per save; these files are
  shared by the mod across saves (arkhipelagos: stored solo worlds and
  cached player data).
- **Quota (approval note): 16 MiB per mod**, counted over all its files.
  A write that would pass it returns -1 and changes nothing (the old file
  stays whole), so a mod cannot fill a phone's or a console's storage.
  `mod_file_list` also gives the bytes used and the quota. A file is
  written to a temporary name and renamed, so a crash never leaves half a
  file.
  - Why 16 MiB is enough for arkhipelagos: a stored world is its
    configuration, seed and generator version (tens of KB; the world is
    made again from them, see `docs/metaxy-proposal.md` C4), so hundreds
    of worlds fit.

## 16. metaxy world sessions and the Archipelago bridge

The port speaks to metaxy (a wasm mod has no sockets), so these are
services. The frames are in `docs/metaxy-proposal.md`.

```c
int  (*world_create)(const char *config_json, u32 *ticket);      /* generate on metaxy */
int  (*world_owner)(const char *code, const char *owner_key, u32 action); /* start, close, delete */
int  (*world_join)(const char *code, const char *name_or_key, u32 *ticket);
int  (*world_checks)(const u32 *location_ids, u32 count);          /* sent again on every connect */
void (*world_leave)(void);
void (*on_world)(BwWorldHandler fn);  /* generated (code, owner key), joined (player data, key),
                                         items (with index), positions, goals, errors */

/* room: an archipelago.gg room link, or host:port. With no host given, the
   port uses its Archipelago server setting: archipelago.gg by default; the
   player changes it in the settings screen, beside the relay address. */
int  (*ap_link)(const char *room, const char *slot, const char *password, u32 *ticket);
int  (*ap_send)(const char *json, u32 size);
void (*on_ap)(BwApHandler fn);         /* Archipelago packets, slot_data pushed at once */
```

## 17. Other players in the field and on the Town Map

```c
int (*peer_position)(int on);   /* the port sends this game's zone and tile to metaxy, and
                                   draws other players of the world session as field avatars
                                   in the same zone (the avatar services exist) */
int (*townmap_marker_set)(u32 id, u16 zone, u32 icon, const u16 *name);
int (*townmap_marker_clear)(u32 id);
```

- Field: avatars only in the same zone (zone changes are rare, tile
  positions a few per second only while someone shares the zone).
- Town Map: the port turns a zone into its Town Map place with the game's
  own tables (`GAME_BEACON_ACCESSOR_GetTownmapRootZoneId` 0x02165D34 and
  the Town Map spot data) and draws the markers on the game's Town Map
  (ov072, `TOWNMAP_Create` 0x021B95A0, `TOWNMAP_Update` 0x021B98F8) and on
  eidos's own one-screen Town Map.

## Order of building (when approved)

1, 2, 3, 15 first (they let the mod put its content into a game); then 5,
6 to 13 (the options); then 14, 16, 17 (online play). Each one with its
test in eidos's own suite before the next.

## Decided

1. Everything ships at once, as interface version 6.
2. The memory cap: 16 MiB per mod, 32 MiB for all mods (reasons in
   item 1).
3. Approved on 2026-11-16 with three notes, now in the items:
   - the type chart is written once with `type_chart_set` (main path); the
     per-lookup event stays for rule mods (item 11);
   - member overrides also reach archives that are already open, through
     a layout-stable image swapped in between frames (item 1);
   - mod files have a quota of 16 MiB per mod (item 15).

## Additions requested

Found while building the mod (2026-11-16); not part of the approved text.

1. `game_code(char out[4])`, state: the cartridge's game code ("IRBO",
   "IRAJ" and so on), from `bw_lang_gamecode()`. The mod needs the version
   (Black or White) and the language family to make a world and pick the
   script table. eidos keeps both in host memory (`g_pm_version`), so a mod
   cannot read them; the header copy at 0x027FFE00 is not filled by the
   port. Until it exists the mod tries that copy, then takes the language
   from `text_language` and assumes Black.
