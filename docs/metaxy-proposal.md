# Proposal: metaxy world sessions and Archipelago bridge

Approved on 2026-11-16, with two notes that are now written in: a world
is stored as its configuration, seed and generator version, not as the
generated world (C4), and the world store must survive restarts and
redeploys (Part F). It is written as new parts of metaxy's `docs/SPEC.md`,
in its style. The metaxy work itself is done in the metaxy repository; this
document is the agreed design.

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
- **What `world.json` holds (approval note).** The generator is
  deterministic: the same configuration and seed give the same world,
  checked between native and wasm32 builds. So metaxy stores what makes
  the world, not the world:
  - the configuration (as checked on `N`),
  - the seed,
  - the generator version (`arkh-gen` crate version and its data hash),
  - the world hash the generator gave at creation.
- **On load** metaxy makes the world again with that generator version
  and compares the hash. Same hash: the world is served. Another hash:
  the world is not served (`E gen-mismatch`) and the operator log says
  which world and which version; nothing is guessed. The stored hash is
  what guards against a generator change.
- A world is made again only when it is first needed after a start (a
  join or a check), then kept in memory while it has players, and dropped
  after 10 minutes without one. 16 players take about 1.6 s.
- The log stays as it is: it is the world's state (claims, checks, items
  handed over, goals), which the generator cannot make again.
- At start, metaxy rebuilds a world's state from `world.json` and `log`;
  logs are compacted from time to time.
- Size: `world.json` is tens of KB (the configuration is at most 64 KB),
  instead of about 2 MB for a generated 16-player world. The log grows
  with play: about 1,200 checks per player, a line each, so a few hundred
  KB for a full 16-player world at its end.
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
| `gen-mismatch` | the world made again on load does not match its stored hash |

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
- A world records the generator version and its world hash (C4).
  Because metaxy makes stored worlds again on load, a newer generator
  must not drop the version old worlds need. metaxy links every generator
  version that open worlds use (Cargo allows one crate at several versions
  under different names: `arkh-gen-0-1`, `arkh-gen-0-2`), and a world is
  made by the version it names.
- A version is removed only when no open world uses it (worlds expire
  after at most 90 days without a join). Before that, an operator command
  `metaxy world freeze <version>` writes the generated world of each
  remaining world into its folder (`world.full.json`, the old 2 MB form),
  and metaxy serves those from the file.
- Inside one version, a change that alters worlds is a bug: arkhipelagos'
  golden hashes catch it before a release tag.
- metaxy's tests include arkhipelagos' golden hashes for the pinned tag, so
  a dependency update that changes worlds is noticed.
- The world format and the configuration format are owned by arkhipelagos
  (`docs/generator.md` there); metaxy only stores and routes them.

## Part F: deployment

The world store is state that must outlive the process (approval note).

- `--world-store <dir>` names a folder that survives restarts and
  redeploys. metaxy refuses to start the world role when the folder cannot
  be written, and says so; it never falls back to a temporary folder.
- **The public relay runs on Railway (decided).** The world store is a
  Railway Volume attached to the metaxy service, mounted at `/data`, and
  metaxy runs with `--world-store /data/worlds`. There is no separate
  storage service.
  - A Railway service with a volume runs as a single instance. That fits
    metaxy: it is one process and keeps open worlds in memory, so two
    instances must never share a store anyway.
  - A redeploy stops the old instance before the new one mounts the
    volume, so there is a short gap. metaxy's SIGTERM handling (below)
    flushes every log first, and clients reconnect and resend their
    checks (C2), so nothing confirmed is lost.
  - The image runs as the non-root user `metaxy`, and a Railway volume is
    mounted owned by root. At set-up, either set `RAILWAY_RUN_UID=0` on
    the service (Railway's own advice for non-root images) or give the
    folder to the user; the start check above refuses to run the world
    role if `/data/worlds` cannot be written, so a wrong set-up shows at
    once.
  - Size: start the volume at 1 GB (below) and grow it in Railway when
    the operator log's store size nears it.
- **Docker** (self-hosted): the store is a named volume, never the
  container's own layer, which a redeploy throws away:

  ```sh
  docker volume create metaxy-worlds
  docker run -d --restart unless-stopped -p 4141:4141 \
    -v metaxy-worlds:/var/lib/metaxy/worlds metaxy --world-store /var/lib/metaxy/worlds
  ```

  The image does not declare a `VOLUME` (Railway does not take one); the
  README shows the named-volume form.
- **systemd:** the store is `/var/lib/metaxy/worlds` with
  `StateDirectory=metaxy/worlds`, so it is made with the right owner and
  kept across upgrades.
- **Other hosts:** a persistent volume mounted at the store path. One
  metaxy instance per volume: two instances must never share one store,
  as each keeps worlds in memory.
- **Size:** at tens of KB per world plus its log, 10,000 open worlds take
  a few GB at most. 1 GB is enough to start.
- **Backups:** the store is plain files, written with flush and rename, so
  a file copy of the folder while metaxy runs is a usable backup (the last
  log line may be cut; metaxy skips a cut last line on load).
- Shutting down: on SIGTERM metaxy stops taking checks, flushes every log
  and exits, so a redeploy loses nothing confirmed.

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
- Approved on 2026-11-16 with two notes, now in the text:
  - a world is stored as configuration, seed, generator version and world
    hash; metaxy makes it again on load and checks the hash; the log stays
    (C4, Part E);
  - the world store survives restarts and redeploys: a persistent volume
    in a container (Part F).
- The public relay runs on Railway: a Railway Volume on the metaxy
  service, mounted at `/data`, with `--world-store /data/worlds`; no
  separate storage service; a service with a volume is a single instance,
  which fits metaxy's one-process design (Part F).
- The world store on the public relay's disk: yes, at tens of KB per world.
