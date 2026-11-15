# Proposal: metaxy world sessions and Archipelago bridge

For the user's approval. Nothing here is built, and nothing is changed in
the metaxy tree, until it is approved. It is written as new parts of
metaxy's `docs/SPEC.md`, in its style.

metaxy stays one service, one process. It gains two roles beside the
relay:

- **world sessions** (eidos-only games): generate worlds, keep them and
  their state, push each player's data on join, route items, queue items
  for absent players, pass positions;
- **Archipelago bridge**: one outbound WebSocket per bridged player to an
  Archipelago server, room-link look-up, messages piped both ways.

The relay part keeps its rules (forwards, keeps nothing). The world-session
role keeps worlds on disk; this is new, and the specification says so
plainly.

## Part C: world sessions

### C1. What it does

- Generates a world from a game configuration, with arkhipelagos' generator
  linked in (`arkh-gen`, Rust, no Python).
- Keeps each world and its state on disk until it expires.
- Lets players join any time, without waiting for anyone. There is no host.
- On join, pushes the player's whole data at once, then every item waiting
  for them.
- Records checks, gives each item to its receiver now or queues it.
- Passes positions between players of a world (field and Town Map).

### C2. Frames

The format is the relay's (`<type> <length>\n<payload>`). New types:

| Direction | Type | Payload | Meaning |
|---|---|---|---|
| client | `N` | `<version> <token>\n<configuration JSON>` | create and generate a world |
| relay | `R` | `<code> <owner key>` or `wait <position>` | generated (the world's code and the creator's owner key), or the place in the queue |
| client | `O` | `<version> <code> <owner key> <action>` | owner action: `start`, `close`, `delete` |
| client | `I` | `<version> <token> <code> <name or player key>` | join a world session |
| relay | `S` | `<player> <player key>\n<player data JSON>` | the push on join: player number, key (new on a first join), the player's whole data |
| client | `V` | `<location id> ...` or `goal` | checks (sent again on every connect) or the goal reached |
| relay | `Y` | `<index> <item id> <from player> ...` | items for this player, with their index |
| client | `U` | `<zone> <x> <y> <z> <dir>` | this player's position (zone changes; tile positions only while someone shares the zone) |
| relay | `U` | `<player> <zone> <x> <y> <z> <dir>` | another player's position |
| both | `F` | `<message id> <last 0/1>\n<bytes>` | a piece of a big payload (below) |

`W`, `P` and `X` keep their meaning in a world session (you are in; a player
came; a player left). `N`, `O` and `I` carry `eidos-net/2`; a relay that
speaks only version 1 answers `E version`, so the game knows.

**Big payloads.** A configuration, a player's data (about 50 to 120 KB) and
an Archipelago message can be larger than a frame. They travel as `F` frames
with one message id; the frame that uses them carries `@<message id>` as
its payload part. Limits per kind: configuration 64 KB, player data 1 MB,
Archipelago message 2 MB.

### C3. A world's life

1. `N`: the configuration is checked (schema, 1 to 16 players, sizes) and
   queued; `R wait <n>` while it waits; then `R <code> <owner key>`. The
   world is stored. Nobody can join yet.
2. `O start` (owner key): joinable.
3. `I` by name: the name must be one of the world's players and not yet
   claimed; metaxy makes a player key, stores its hash, and pushes
   `S <player> <key>` with the data. Later joins use the key, so nobody can
   take a claimed slot.
4. Play: `V` checks; metaxy looks up the placement and appends the item to
   the receiver's list (`Y` at once if they are connected). A check for the
   player's own item is recorded too (the game gave it already).
5. Leaving and joining again cost nothing: metaxy keeps the world.
6. `O close`: no more joins; `O delete`: gone at once.

### C4. The store

- A folder per world: `world.json` (written once) and `log` (append-only,
  one line per change: claim, check, item handed over, goal). Each line is
  written and flushed to disk before metaxy answers, so a crash loses
  nothing that was confirmed.
- At start, metaxy rebuilds a world's state from `world.json` and `log`;
  logs are compacted from time to time.
- Size: a 16-player world is about 1 to 2 MB.
- Expiry (decided): deleted after 90 days with no join, or 30 days after
  every player reached the goal. Only the owner key deletes earlier.
- What it holds about people: the names in the configuration, key hashes,
  last-seen times. No addresses. The privacy section of SPEC.md gains this.

### C5. Generation

- `arkh-gen` is a Rust crate linked into metaxy. Worlds are made on a small
  thread pool (setting `gen-workers`, default the number of cores minus
  one), never on the async runtime, so relay traffic never waits on it.
- A generation has a time limit (default 10 s; 16 players take about 1.6 s
  today) and the configuration a size limit.
- Same configuration and seed give the same world (checked across native
  and wasm32 builds).

### C6. Limits (defaults; all settings)

| Limit | Value | When reached |
|---|---|---|
| Generations per address | 6 per hour | `E busy` |
| Queued generations | 64 | `E gen-busy` |
| Worlds owned per owner key holder (per address) | 20 open | `E busy` |
| Players in a world | 16 | (configuration refused) |
| Positions | about 8 per second while someone shares the zone | excess dropped |

Rates per client count all roles together.

### C7. New error reasons

| Reason | Meaning |
|---|---|
| `gen-failed` | the configuration cannot make a world (the text says why) |
| `gen-busy` | the queue is full |
| `no-world` | no world has that code |
| `not-started` | the world is not joinable yet |
| `closed` | the owner closed the world |
| `not-owner` | the owner key is wrong |
| `name-taken` | that player is claimed; join with the player key |
| `bad-key` | the player key is wrong |

## Part D: the Archipelago bridge

### D1. What it does

- One outbound WebSocket (wss or ws) per bridged player to an Archipelago
  server; Archipelago messages (JSON text) pass both ways as `T` frames.
- metaxy does not read Archipelago messages except to check that the
  server's first one is `RoomInfo`; a link that does not answer the
  WebSocket upgrade and send `RoomInfo` is closed (no open proxy).
- The player's `slot_data` arrives in Archipelago's `Connected` message and
  goes to the game as one `T` message (in `F` pieces): the push at once.

### D2. Frames

| Direction | Type | Payload | Meaning |
|---|---|---|---|
| client | `L` | `<version> <token>\n<room link or host:port>` | open the link |
| relay | `L` | `ok` | linked (after `RoomInfo`) |
| both | `T` | `<JSON text>` or `@<message id>` | one Archipelago message |

### D3. Which server (decided)

- By default the bridge connects to archipelago.gg (a room link from that
  site, or `archipelago.gg:<port>`).
- The player can change it to any other Archipelago server, as a setting
  in eidos, the same way they can change the metaxy relay address. The
  game sends the host and port in `L`; metaxy connects to what it is given.
- Whatever the host, the open-proxy guard stays: a link that does not
  answer the WebSocket upgrade and send `RoomInfo` as its first message is
  closed, and nothing the player sent is passed on before that.

### D4. Room links

archipelago.gg rooms sleep after 2 hours without activity and come back on
another port. For a room link, metaxy reads `/api/room_status/<room>`
(`last_port`), opening the room page first to wake it, then connects. For
`host:port` it connects directly.

### D5. Limits and errors

| Limit | Value |
|---|---|
| Bridge links per address | 4 |
| Archipelago message size | 2 MB (in `F` pieces) |
| Link set-up time | 15 s |

Errors: `ap-unreachable` (no connection, no WebSocket, no `RoomInfo`),
`ap-closed` (the server closed the link).

### D6. Privacy

The Archipelago server sees metaxy's address, not the player's. Many
players arriving from one address may meet a server's limits; the
operators of archipelago.gg should be asked before release.

## Part E: in the code

| Module | What |
|---|---|
| `codec` (today) | the new frame types and `F` pieces |
| `registry` (today) | connections and limits, shared by all roles |
| `world::store` | folders, the log, compaction, expiry |
| `world::session` | joins, keys, routing checks to items, positions |
| `world::gen` | the generation pool around `arkh-gen` |
| `bridge::ws` | the WebSocket client (on tokio-rustls, which metaxy already uses) |
| `bridge::room` | the room-status look-up (a small HTTP GET) |

New settings: `--world-store <dir>` (off when empty: the relay alone, as
today), `--gen-workers`, `--bridge on/off`.

### The generator dependency

metaxy depends on the generator crate from the arkhipelagos repository,
pinned to a release tag:

```toml
[dependencies]
arkh-gen = { git = "https://github.com/Project-Holon/arkhipelagos", tag = "gen-v0.1.0" }
```

- The crate carries the game's logic compiled in; metaxy reads no data
  files for it.
- A world records the generator version and the data hash. metaxy serves a
  stored world from its stored data, so an older world keeps working when
  metaxy moves to a newer generator.
- metaxy's tests include arkhipelagos' golden hashes for the pinned tag, so
  a dependency update that changes worlds is noticed.
- The world format and the configuration format are owned by arkhipelagos
  (`docs/generator.md` there); metaxy only stores and routes them.

## Tests

- Every frame: parse and refuse cases, as `tests/` does today.
- World store: write, crash in the middle of a write, rebuild, compaction,
  expiry.
- Sessions: two players join at different times, items queue and arrive
  with the right index, a check sent twice counts once, a claimed name
  needs the key.
- Generation: the hash of a test world matches `arkh-gen`'s golden file.
- Bridge: against a local Archipelago server (MultiServer) and a fake
  server that does not send `RoomInfo`.

## Decided

- The bridge connects to archipelago.gg by default; the player may set any
  other Archipelago server in eidos (D3). The `RoomInfo` guard stays.

## Questions

1. The world store needs a disk on the metaxy machine (worlds are about
   2 MB each). Is that fine for the public relay?
