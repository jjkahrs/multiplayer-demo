# Task Plan: 150-Player Zone Sync Demo

Source design: [TECHNICAL_DESIGN.md](./TECHNICAL_DESIGN.md)
Source requirements: [REQUIREMENTS.md](./REQUIREMENTS.md)

Build/test commands (run from `dumb-server/` unless stated):
- `cargo build` / `cargo test` — whole workspace
- `cargo run -p server` — run the server (binds `0.0.0.0:8080` by default)
- `cargo run -p bot -- --clients N --duration S` — run the bot loader
- `docker compose -f docker/docker-compose.yml up --build -d` — full stack (MySQL + server)
- `curl.exe http://127.0.0.1:8080/health` — health check (use `curl.exe`; PowerShell aliases `curl` to `Invoke-WebRequest`)
- `docker compose -f docker/docker-compose.yml exec mysql mysql -udemo -pdemo demo -e "SHOW TABLES"`

Notes that apply across the whole plan:
- **DB-gated tests skip cleanly.** Persistence tests only run when `TEST_DATABASE_URL` is set; without it they return early. Plain `cargo test` must always pass without a database running.
- **Unity tasks are agent-driven through Unity MCP** (port 8077; the game server keeps 8080). The agent writes C#, authors scenes/prefabs/controllers, enters play mode, drives the client in-engine (`execute_code`: UI fields, button invoke, synthetic Input System keyboard events — never the OS mouse/keyboard), reads the console, runs EditMode tests, and builds the player. Every Unity task follows TECHNICAL_DESIGN.md → *Editor automation & verification loop*.
- **Screenshots:** MCP writes only inside the Unity project → capture to `dumb-unity-client/Captures/<task>-<before|after>.png`, copy to `docs/screenshots/`. Every Unity task has a before/after pair.
- **Unity tests:** MCP `run_tests` EditMode, assembly `Demo.Tests.EditMode`. No PlayMode test assembly; play-mode behavior is checked through MCP.
- **Console noise to ignore:** `generators.ai.unity.com` `NoSubscription` errors (AI Assistant package, unrelated).
- **Two rendered clients** are never tested with two Unity editor instances (heavy/flaky). The standalone Windows build (T8.1) is the second client.

## Progress
- [x] Phase 1 — Protocol core (Rust workspace + JSON protocol)
  - [x] T1.1 — Scaffold workspace and implement the protocol crate
- [x] Phase 2 — Live in-memory server
  - [x] T2.1 — Server crate skeleton: axum, config, /health, /metrics, /ws echo
  - [x] T2.2 — Zone core: World, Player FSM, 20 Hz tick, snapshot broadcast
  - [x] T2.3 — Live wiring: ingest + writer, end-to-end integration test
- [x] Phase 3 — MySQL persistence
  - [x] T3.1 — docker-compose (MySQL 8.4 + server), init.sql schema
  - [x] T3.2 — sqlx pool + profile repository + rejoin-restores test
- [x] Phase 4 — Harden server behavior
  - [x] T4.1 — Grace period + playerJoined/playerLeft events
  - [x] T4.2 — Duplicate names + invalid-input tolerance
- [x] Phase 5 — Bot loader + metrics + 150-player run
  - [x] T5.1 — Bot crate: CLI, bots, one-way latency measurement, report
  - [x] T5.2 — Server /metrics with CPU/RAM + periodic log
  - [x] T5.3 — 150-bot acceptance run + docs/demo-run.md
- [ ] Phase 6 — Unity networking
  - [x] T6.1 — Verify join flow via MCP; remove editor-control workarounds
- [x] Phase 7 — Unity world rendering
  - [x] T7.1 — Demo.unity scene + CameraFollow
  - [x] T7.2 — PlayerAvatar AnimatorController + prefab + name label
  - [x] T7.3 — ZoneView: spawn players, apply snapshots, interpolate
  - [x] T7.4 — MovementInput: WASD to input messages
- [ ] Phase 8 — Acceptance + demo polish
  - [x] T8.1 — Launch-arg auto-join + standalone Windows build via MCP
  - [x] T8.2 — Two-rendered-client acceptance (A moves, B sees it)
  - [x] T8.3 — Full 150-player acceptance + final demo-run.md
  - [x] T8.4 — Demo polish + one-command bring-up

---

## Phase 1 — Protocol core

**Goal:** A cargo workspace under `dumb-server/` with a `protocol` crate defining every message type from the design, name validation, and serialization round-trip tests.
**Exit condition:** `cargo test -p protocol` passes and serialized JSON matches the design's examples byte-for-byte.

### [ ] T1.1 — Scaffold workspace and implement the protocol crate

**Depends on:** none
**Files:** `dumb-server/Cargo.toml` (new, workspace), `dumb-server/crates/protocol/Cargo.toml` (new), `dumb-server/crates/protocol/src/lib.rs` (new), `dumb-server/crates/protocol/src/messages.rs` (new), `dumb-server/crates/protocol/src/validation.rs` (new), `dumb-server/crates/protocol/tests/serde.rs` (new)

**Context:** Nothing exists under `dumb-server/`. Later phases add the `server` and `bot` crate members to this workspace; this task only creates the `protocol` member.

**Do:**
- Workspace manifest with `members = ["crates/protocol"]` (add other members in later tasks). Cargo package name for the shareable crate: `protocol`.
- `messages.rs` — Serde structs/enums matching the design's JSON exactly (use `#[serde(rename_all = "camelCase")]` + `#[serde(tag = "type")]` for server→client and client→server enums so the wire name is `type` and values are lowercase like "join"):
  - Client → Server: `Join { name }`, `Input { vx, vz, seq, t0 }`, `Leave`.
  - Server → Client: `Joined { player_id, name, x, z, yaw }`, `Snapshot { players: Vec<SnapshotPlayer> }` where `SnapshotPlayer { id, name, x, z, yaw, state, seq, t0 }` and `state` is an enum `"idle" | "walk"`, `PlayerJoined { id, name }`, `PlayerLeft { id }`, `ErrorMsg { code, message }`.
  - User-facing naming inside Rust is snake_case; serde's `rename_all` handles the wire format. All numeric fields are `f64` (x, z, yaw, vx, vz) and `u64`/`i64` for ids/seq/t0 as appropriate — pick and be consistent; document in doc comments.
- `validation.rs` — `sanitize_name(&str) -> Result<String, NameError>`: trim whitespace, reject if empty, reject if longer than 16 chars, reject if it contains any control char (use `char::is_control`). `NameError` is a simple enum (`Empty`, `TooLong`, `IllegalChar`) carrying the reason.
- Separate the `snapshot` entry from `joined` shaping (design shows `playerId` in `joined` but `id` in snapshot players — this is deliberate; keep them as documented).
- Tests in `tests/serde.rs`: round-trip every message via `serde_json` (to value and back), non-default fields, a snapshot with 1 and with 3 players, and exact-string assertions for at least the `join`, `joined` and a single-player `snapshot` against the JSON shown in the design (bytes unchanged). Validation tests: `"  Bob  " → "Bob"`, `""` and `"   "` and 17-char and `"\n"` names rejected, 16-char name accepted, a control-char name rejected.

**Acceptance:**
- [ ] `cargo build` and `cargo test -p protocol` pass.
- [ ] The three serialization exact-string tests match the design's example JSON byte-for-byte.
- [ ] Every documented name-validation case has a passing test.
- [ ] `cargo metadata` shows `protocol` as the sole workspace member.

---

## Phase 2 — Live in-memory server

**Goal:** An axum server that accepts WebSocket connections; a Zone task owners world state, integrates movement at 20 Hz, and broadcasts one pre-serialized snapshot per tick; join/input/leave work end-to-end from a real WebSocket client. No database yet.
**Exit condition:** `cargo test -p server` passes including an end-to-end test where a client joins, moves, and its position advances in snapshots.

### [ ] T2.1 — Server crate skeleton: axum, config, /health, /metrics, /ws echo

**Depends on:** T1.1
**Files:** `dumb-server/Cargo.toml` (add `crates/server` to members), `dumb-server/crates/server/Cargo.toml` (new), `dumb-server/crates/server/src/main.rs` (new), `dumb-server/crates/server/src/config.rs` (new), `dumb-server/crates/server/src/http.rs` (new), `dumb-server/crates/server/tests/echo.rs` (new)

**Context:** The `protocol` cate exists. The server crate does not yet exist. This task gives the server a runnable shell; the Zone is added in T2.2.

**Do:**
- Dependencies (latest stable; known-good majors): `axum 0.8`, `tokio 1` (features `macros`, `rt-multi-thread`, `net`, `time`, `sync`, `signal`), `serde`, `serde_json`, `tracing`, `tracing-subscriber`; dev-dependencies: `tokio-tungstenite 0.26`, `futures-util`. Add `protocol` as a path dependency (needed for parsing later — wire now so the build is cheap).
- `config.rs` — `Config` struct from env with defaults: `BIND` (`0.0.0.0:8080`), `TICK_HZ` (20), `GRACE_MS` (5000), `SPEED` (5.0 m/s), `WORLD_HALF` (50 m). `DATABASE_URL` is optional and unused until T3.2. Parse with `std::env` + known fallbacks; log effective values on startup.
- `http.rs` — `Router` with `GET /health` → 200 `{"status":"ok"}`, `GET /metrics` → 200 `{}` (stub now), `GET /ws` → accepts the upgrade and echoes back the first text frame it receives (temporary; replaced in T2.3).
- `main.rs` — init `tracing`, read config, build router, bind listener, `axum::serve`, graceful shutdown on ctrl-c (log `shutting down`).
- `tests/echo.rs` — bind a `TcpListener` on `127.0.0.1:0`, `tokio::spawn` `axum::serve`, connect with `tokio-tungstenite` via `async-tungstenite`-style `connect_async`, send `"ping"`, assert a `"ping"` text reply.

**Acceptance:**
- [ ] `cargo test -p server` passes, including the echo test.
- [ ] `cargo run -p server` starts and logs its effective config; `curl.exe http://127.0.0.1:8080/health` returns 200 JSON and `/metrics` returns 200 `{}`.
- [ ] Ctrl-C stops the process with the graceful `shutting down` log line.

### [ ] T2.2 — Zone core: World, Player FSM, 20 Hz tick, snapshot broadcast

**Depends on:** T2.1
**Files:** `crates/server/src/player.rs` (new), `crates/server/src/zone.rs` (new), `crates/server/src/metrics.rs` (new), `crates/server/src/main.rs` (add module decls + spawn Zone)

**Context:** The server shell runs. The Zone task is the single authority over world state (design principle: no shared mutable state; all reads/writes funnel through one async task).

**Do:**
- `player.rs` — `Player { player_id: u64, name: String, x: f64, z: f64, yaw: f64, dir_x: f64, dir_z: f64, seq: u64, t0: u64, status: PlayerStatus, suspend_deadline: Option<Instant> }`. `enum PlayerStatus { Joining, Active, Suspended, Removed }` (FSM from the design). Methods:
  - `apply_input(vx, vz, seq, t0)` — if the magnitude of (`vx`,`vz`) is deliberately out of the `[-1,1]` range, ignore the update entirely (trust level from requirements); else store the normalized direction, `seq`, `t0`.
  - `integrate(dt, speed, world_half)` — `x += dir_x * speed * dt`, `z += dir_z * speed * dt`; clamp both to `[-world_half, world_half]`; when moving, `yaw = atan2(dir_z, dir_x)`; state is `Walk` when magnitude > epsilon else `Idle`.
- `metrics.rs` — `Stats { players: u32, tick_count: u64, last_tick_dt_ms: f64 }` behind an `Arc<Mutex<Stats>>`; Zone updates per tick; `snapshot()` returns a copy for `/metrics`. (CPU/RAM fields are added in T5.2.)
- `zone.rs` — `enum ClientEvent { Join { name, unicast_tx }, Input { player_id, vx, vz, seq, t0 }, Closed { player_id } }` and `enum Outbound { Snapshot(Arc<[u8]>), Event(ServerMsg) }`. Zone task holds the `mpsc::Receiver<ClientEvent>`, a `tokio::sync::broadcast::Sender<Outbound>` (capacity ~4), a `u64` player-id counter, `HashMap<u64, Player>`, and the metrics handle. Loop with `tokio::select!` over incoming events and a `interval(Duration::from_secs_f64(1.0/TICK_HZ))` tick. Each tick: integrate all Active players (Suspended stay put), build the `ServerMsg::Snapshot` for all non-Removed players (Suspended render with `state="idle"`), serialize once to bytes, send as `Outbound::Snapshot(Arc::from(vec))`, bump metrics. Join → assign a fresh monotonic `player_id`, insert at spawn `(0,0)` with `yaw` facing world center, send the unicast `ServerMsg::Joined`. Closed → set `Removed` and drop from the world immediately (grace comes in T4.1; note this evolution to keep code simple now).

**Acceptance:**
- [ ] `cargo test -p server` passes, with unit tests: bound clamping at `±50`, yaw correct for direction, out-of-range input vector is ignored (position unchanged), Idle/Walk state derivation.
- [ ] One broadcast snapshot per tick is emitted containing all non-Removed players (assert count over a short window in a unit test).

### [ ] T2.3 — Live wiring: ingest + writer, end-to-end integration test

**Depends on:** T2.2
**Files:** `crates/server/src/ingest.rs` (new), `crates/server/src/writer.rs` (new), `crates/server/src/http.rs` (wire `/ws`), `crates/server/tests/zone_e2e.rs` (new), `crates/server/src/main.rs` (spawn per-connection tasks)

**Context:** The Zone task and broadcast channel exist. Now real sockets must talk to it.

**Do:**
- Per-connection setup on `/ws` upgrade: create an `mpsc::channel::<ServerMsg>` (per-connection unicast for `joined`/`error`), spawn two tasks:
  - **ingest (read task)** — holds the shared `Sender<ClientEvent>`; loops `socket.recv()`, parses with `protocol` types (`serde_json`), forwards `Join { name, unicast_tx }` / `Input { ... }` / on socket close or `Leave` → `ClientEvent::Closed { player_id }`. `player_id` for Input/Closed comes from a per-connection field set when the Join reply comes back (store `player_id` after sending Join; note the ordering: the Zone assigns the id, ingest learns it from the `joined` message it receives via a small handshaken channel, or simpler: ingest sends `Join` and holds the raw id by matching on the first unicast reply it routes — implement the cleanest option and document it).
  - **writer (write task)** — `select!` on the broadcast `Outbound` receiver (pre-serialized `Arc<[u8]>` → send as Binary/Text) and the per-connection unicast `mpsc::Receiver<ServerMsg>` (serialize `ServerMsg` → send). If a send fails (dead socket), notify the Zone (send `Closed`) and end the task. Broadcast lag is fine: `broadcast::Receiver` yields `Lagged` on overflow — keep sending the newest frame, never block.
- Room for a per-connection `player_id: Option<u64>` set once `joined` is routed — needed so Input/Closed carry the id.
- `tests/zone_e2e.rs` — in-process server: connect client A, send `join "Alice"`, expect a `joined` frame with a numeric `playerId`; then expect `snapshot` frames at ~20 Hz (measure that 10 snapshots arrive in roughly 500 ms); send `input` with `vx=1, vz=0` and assert A's `x` increases across snapshots; close the socket and assert A disappears from subsequent snapshots (immediate removal, grace comes later). Also connect client B and assert B sees A and the two player ids differ.

**Acceptance:**
- [ ] `cargo test -p server` passes including `zone_e2e`.
- [ ] Cross-check against the live server: `cargo run -p server`, then run the e2e path once manually via `websocat`/a scratch tokio client (or rely on the test) — `joined` then snapshots observed.
- [ ] `/metrics` `players` count matches the number of connected clients during the e2e test.

---

## Phase 3 — MySQL persistence

**Goal:** Player profiles (name + last position) are read on join and written on disconnect through a real MySQL server in docker-compose; rejoin restores the saved position.
**Exit condition:** With dockerized MySQL running, the rejoin-restores test passes (`cargo test -p server --test persistence` with `TEST_DATABASE_URL` set).

### [ ] T3.1 — docker-compose (MySQL 8.4 + server), init.sql schema

**Depends on:** T2.3
**Files:** `dumb-server/docker/docker-compose.yml` (new), `dumb-server/docker/init.sql` (new), `dumb-server/Dockerfile` (new), `dumb-server/.dockerignore` (new)

**Context:** The server runs locally. The design makes `docker/init.sql` the single source of truth for the schema (no sqlx migrations; sqlx is runtime-checked so builds never need a live DB).

**Do:**
- `docker/init.sql` — exactly the schema from the design:
  ```sql
  CREATE TABLE profiles (
    id           BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    display_name VARCHAR(16)  NOT NULL,
    pos_x        DOUBLE       NOT NULL DEFAULT 0,
    pos_z        DOUBLE       NOT NULL DEFAULT 0,
    yaw          DOUBLE       NOT NULL DEFAULT 0,
    updated_at   TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
    KEY idx_profiles_display_name (display_name)
  ) ENGINE=InnoDB;
  ```
- `Dockerfile` — multi-stage: builder `rust:1` builds `cargo build --release -p server`, runtime `debian:bookworm-slim` copies the binary, exposes 8080, `ENTRYPOINT` runs it.
- `docker/docker-compose.yml` — no `version:` key (Compose v5). Services:
  - `mysql`: image `mysql:8.4`, env `MYSQL_ROOT_PASSWORD`/`MYSQL_DATABASE=demo`/`MYSQL_USER=demo`/`MYSQL_PASSWORD=demo`, port `3306:3306`, mount `./init.sql:/docker-entrypoint-initdb.d/init.sql:ro` plus a named volume for data, healthcheck `mysqladmin ping -h 127.0.0.1`.
  - `server`: build from `../Dockerfile`, env `DATABASE_URL=mysql://demo:demo@mysql:3306/demo`, `BIND=0.0.0.0:8080`, ports `8080:8080`, `depends_on: mysql: condition: service_healthy`.

**Acceptance:**
- [ ] `docker compose -f docker/docker-compose.yml config` validates.
- [ ] `docker compose -f docker/docker-compose.yml up -d mysql` reaches healthy, and `docker compose -f docker/docker-compose.yml exec mysql mysql -udemo -pdemo demo -e "SHOW TABLES"` lists `profiles`.
- [ ] `docker compose -f docker/docker-compose.yml up --build -d` starts the server; `curl.exe http://127.0.0.1:8080/health` returns 200 and server logs show no DB connection errors.

### [ ] T3.2 — sqlx pool + profile repository + rejoin-restores test

**Depends on:** T3.1
**Files:** `crates/server/Cargo.toml` (add sqlx), `crates/server/src/profile.rs` (new), `crates/server/src/zone.rs` (wire load on join / save on removal), `crates/server/src/config.rs` + `main.rs` (pool creation), `crates/server/tests/persistence.rs` (new)

**Context:** The DB exists in docker-compose. The server currently ignores it. T2.3's removal is immediate (no grace yet) — the rejoin test relies on that, and T4.1 will change removal to a grace period (and updates this test).

**Do:**
- Add `sqlx 0.8` with features `runtime-tokio`, `mysql`. Use runtime-checked `sqlx::query`/`query_as`, never the compile-time macros.
- `config.rs` — `DATABASE_URL: Option<String>`.
- `main.rs` — if `DATABASE_URL` is present, `MySqlPoolOptions::new().connect_lazy(&url)` and pass `Option<MySqlPool>` into the Zone; log clearly when running without persistence.
- `profile.rs` — `load_or_create(&Pool, name) -> (profile_id: u64, spawn: (x, z, yaw))`: `SELECT id, pos_x, pos_z, yaw FROM profiles WHERE display_name = ? ORDER BY updated_at DESC LIMIT 1`; if a row exists return it, else `INSERT INTO profiles (display_name) VALUES (?)` and return the new id with spawn at origin + a random offset `< 2 m` (so bots don't stack). `save(&Pool, profile_id, x, z, yaw)` → `UPDATE profiles SET pos_x=?, pos_z=?, yaw=? WHERE id=?`.
- `zone.rs`: on `Join`, when a pool exists, `await load_or_create(name)` before inserting (only the Zone touches the world, so the DB call lives inside the Zone's Join handler — the design's principle, not on the ingest task). Keep `profile_id` on the `Player`. On removal (drop to `Removed`), when a pool exists, `tokio::spawn` a fire-and-forget `save(...)` so ticks aren't delayed.
- `tests/persistence.rs` — skip cleanly when `TEST_DATABASE_URL` is unset (`if let Ok(url) = env::var(...) else { return }` with a log line). With it set: (1) join "Bob", send `input` up-vector until `z` moves, close, wait until Bob is gone from snapshots, rejoin "Bob", assert `joined` x/z match the saved position within epsilon; (2) two concurrent joins named "Bob" are both accepted with distinct `playerId`s, both visible in snapshots, while sharing one profile row (assert same `profile_id` via SQL after both disconnect).

**Acceptance:**
- [ ] `cargo test -p server` passes with `TEST_DATABASE_URL` unset (persistence tests visibly skip, everything else green).
- [ ] `docker compose up -d mysql`, then `TEST_DATABASE_URL=mysql://demo:demo@127.0.0.1:3306/demo cargo test -p server --test persistence` passes (rejoin restores position; duplicate-name case green).
- [ ] After the run, `SELECT display_name, pos_x, pos_z FROM profiles WHERE display_name='Bob'` shows one row with the last saved position.

---

## Phase 4 — Harden server behavior

**Goal:** Grace-period disconnect, `playerJoined`/`playerLeft` events, duplicate-name robustness, and tolerance of malformed/spammy input. Completes design step 5.
**Exit condition:** All server integration tests green, including grace-timeline, duplicate-name, and junk-input cases.

### [ ] T4.1 — Grace period + playerJoined/playerLeft events

**Depends on:** T3.2
**Files:** `crates/server/src/player.rs`, `crates/server/src/zone.rs`, `crates/server/tests/zone_e2e.rs`, `crates/server/tests/persistence.rs`, `crates/server/src/config.rs` (GRACE_MS already exists)

**Context:** T2.3/T3.2 remove players instantly on close. This task switches to the design's FSM: socket close → `Suspended(deadline = now + GRACE_MS)` → frozen in snapshots as `idle` → removed + `playerLeft` after the deadline. **This intentionally breaks the immediate-removal assertions in `zone_e2e`/`persistence` — update those tests here.** No resume: a socket loss is a new session; a profile row is reused by name only.

**Do:**
- `zone.rs` — `Closed { player_id }` now sets `status = Suspended { deadline: Instant::now() + GRACE_MS }` (from config). In the tick, before building the snapshot, remove any player whose deadline passed: `Outbound::Event(ServerMsg::PlayerLeft { id })` via the broadcast, persist position (fire-and-forget `save` if a pool exists), drop from the world. Suspended players are broadcast as `state="idle"` at their frozen position and skipped by `integrate`.
- Emit `Outbound::Event(ServerMsg::PlayerJoined { id, name })` on every successful Join (after the insert), alongside the snapshot, so other clients can react early. (The receiving client may ignore it — snapshot reconcile is sufficient; keeping the events keeps the protocol contract per requirements.)
- Update tests: `GRACE_MS` is env-configurable; in integration tests set a short value (e.g. `GRACE_MS=200`) via env or config override so tests stay fast. `zone_e2e`: after close, assert the player is STILL in snapshots (idle) during grace, then gone after; assert a `playerLeft` frame is received. `persistence` rejoin test: it needs the player actually removed before rejoining — keep `GRACE_MS=200` so removal is quick, and rejoin after the grace window.

**Acceptance:**
- [ ] `cargo test -p server` green with the updated grace-timeline assertions (present during grace → absent after → `playerLeft` received).
- [ ] A peer client receives `playerJoined` when a new player joins and `playerLeft` when one leaves.
- [ ] Player ids are monotonic and never reused across sessions (assert max id only increases).

### [ ] T4.2 — Duplicate names + invalid-input tolerance

**Depends on:** T4.1
**Files:** `crates/server/src/ingest.rs` (use `protocol::sanitize_name`, error rejects), `crates/server/src/zone.rs` (input guards + rate cap), `crates/server/tests/zone_e2e.rs` (new cases)

**Context:** Names were accepted as-is; now they must be validated per requirements, and the server must survive hostile input (design edge cases: bad name lengths, control chars, junk JSON, out-of-range vectors, input spam).

**Do:**
- Ingest: run `protocol::sanitize_name` on `join`; on failure, send the per-connection unicast `ServerMsg::ErrorMsg { code: "bad_name", message }` and do **not** forward a `Join` to the Zone (socket stays open). On unparseable JSON or an unknown `type`, send `ErrorMsg { code: "bad_message", message }` and drop the frame — never panic, never close the socket.
- Zone `apply_input`: already ignores out-of-range vectors (T2.2). Add a per-player rate cap: ignore `Input` frames that arrive less than ~10 ms after the previous accepted one (i.e. ≤ 100 inputs/s per player); this is a plain timestamp check, no timers.
- Update `zone_e2e` with: `"  a  "` joins as `"a"`; `""`, 17-char, and control-char names → `bad_name` error and no world entry; a garbage JSON frame → `bad_message` and the socket still delivers later frames; `vx=1.5` input ignored (position unchanged); a 500-frame input burst → position advances but no panic or hang; two "Bob"s both accepted with distinct ids and both in snapshots.

**Acceptance:**
- [ ] `cargo test -p server` green with all the new cases above.
- [ ] `cargo run -p server` handles an interactive session sending a garbage line mid-connection without closing the connection.

---

## Phase 5 — Bot loader + metrics + 150-player run

**Goal:** A Rust CLI that provisions N headless players, moves them, measures one-way end-to-end latency via the t0 echo, and proves 150 concurrent players hold with p95 < 100 ms.
**Exit condition:** A recorded 150/150 run in `docs/demo-run.md` with p95 < 100 ms.

### [ ] T5.1 — Bot crate: CLI, bots, one-way latency measurement, report

**Depends on:** T4.2
**Files:** `dumb-server/Cargo.toml` (add `crates/bot` member), `dumb-server/crates/bot/Cargo.toml` (new), `dumb-server/crates/bot/src/main.rs` (new), `dumb-server/crates/bot/src/bot.rs` (new), `dumb-server/crates/bot/src/report.rs` (new)

**Context:** The server accepts many players and echoes each player's newest `t0`/`seq` in snapshots. The bot crate shares `protocol` types with the server. Note the futures/tungstenite types on the client side are `tokio-tungstenite`'s, which is the same ws implementation axum uses, so text/binary handling matches the server exactly.

**Do:**
- `bot/Cargo.toml` deps: `tokio` 1, `tokio-tungstenite` 0.26, `futures-util`, `serde`/`serde_json`, `clap` 4, `rand`; path dep `protocol`.
- `main.rs` — clap CLI: `--server` (default `ws://127.0.0.1:8080/ws`), `--clients` (default 1), `--duration` seconds (default 30), `--move random` (only mode), `--json <path>` optional to write the report as JSON. Spawn `--clients` bot futures concurrently, run for `--duration`, collect each bot's stats.
- `bot.rs` — one bot: connect via `tokio-tungstenite`, send `join("Bot-<i>")`, wait for the `joined` frame (captures its own `playerId`); then loop at 10 Hz: pick a random normalized direction, re-roll the direction every 1–3 s, send `input { vx, vz, seq+=1, t0 = monotonic ms }`; on every incoming `snapshot`, count it (recv rate) and, for **each entry that is not itself whose `seq` advanced since the previous snapshot**, sample `one_way = now_ms - entry.t0` into the bot's latency list. (Decided in T5.1: sampling every entry re-measures stale `t0`s and inflates p95 past 100 ms by construction. `t0`/`now_ms` are Unix-epoch ms so any process on the machine shares the clock.) Also detect forced disconnect (server dropping the bot) — count any unexpected close as a failure.
- `report.rs` — aggregate across bots: `connected/total`, snapshots/s per client (avg), one-way latency `avg` and `p95` across all samples, and (if enabled) fetch `http://<server>/metrics` once at the end to include server CPU/player counts. Print a readable table to stdout; if `--json` given, write the same data as JSON.

**Acceptance:**
- [ ] `cargo run -p bot -- --clients 10 --duration 15` against a local server finishes without crashing and prints: `10/10 connected`, recv rate ≈ 19–20 Hz, latency avg/p95 numbers.
- [ ] The same run with a name-loading server (DB up) works identically.
- [ ] Forced-drop detection: killing the server mid-run reports the bots as disconnected rather than hanging forever.

### [ ] T5.2 — Server /metrics with CPU/RAM + periodic log

**Depends on:** T4.2 (independent of T5.1; safe to run in parallel with it)
**Files:** `crates/server/Cargo.toml` (add `sysinfo`), `crates/server/src/metrics.rs`, `crates/server/src/http.rs`, `crates/server/src/main.rs`

**Context:** `/metrics` currently returns `{}`. The bot report and `docs/demo-run.md` want server CPU/RAM numbers.

**Do:**
- Add `sysinfo` (pinned `0.38` — `0.39.x` requires Rust 1.95, local toolchain is 1.90; works on Windows and in the docker container). `cpuPercent` is per-core scale (100 = one full core). In `metrics.rs`, hold a `System`/`Process` handle and sample on each `/metrics` read: `cpu_percent`, `memory_bytes` for the server process. Extend `Stats` with `cpu_percent`, `mem_bytes`, `snapshots_per_sec` (computed from tick count over a rolling window), and player count. `/metrics` returns these plus the existing fields.
- `main.rs` — a `tokio::spawn` that every 5 s logs a single `[metrics]` line: players, tick load/avg dt, snapshots/s, cpu%, mem.

**Acceptance:**
- [ ] `curl.exe http://127.0.0.1:8080/metrics` returns JSON containing `players`, `cpuPercent`, `memBytes`, `snapshotsPerSec`.
- [ ] Server logs a `[metrics]` line roughly every 5 s.

### [x] T5.3 — 150-bot acceptance run + docs/demo-run.md

**Depends on:** T5.1, T5.2
**Files:** `docs/demo-run.md` (new)

**Context:** Bot loader and metrics are ready. This is the first real proof of the primary requirement (150 players, one zone).

**Do:**
- Start the stack (`docker compose -f docker/docker-compose.yml up --build -d`). Record `Get-ComputerInfo` essentials for the acceptance run header: OS, CPU name, cores, RAM.
- Run `cargo run -p bot -- --clients 150 --duration 60 --json demo-run-150.json`. Confirm: 150/150 connected, no forced drops, per-client recv rate, p95 one-way latency (must be < 100 ms per requirements; record the actual number). Pull `/metrics` at peak.
- Write `docs/demo-run.md`: header (date, machine specs, server version/commit-less), commands used, results table (connected, recv rate avg, latency avg + p95, server cpu% + mem at peak, snapshot bytes/s), and explicit caveats: CPU% is host-level when containerized and per-core scale (100 = one full core, can exceed 100); t0-echo is only cross-machine-meaningful on shared clocks (single machine here); any deviation from the 100 ms target noted verbatim.

**Acceptance:**
- [ ] 150/150 connected with zero forced drops during the 60 s run.
- [ ] Per-client recv rate ≈ 19–20 Hz.
- [ ] Recorded p95 < 100 ms (actual value in the file).
- [ ] `docs/demo-run.md` present with machine specs, commands, results, and caveats.

---

## Phase 6 — Unity networking

**Goal:** The Unity client connects, joins with a display name, and receives `joined` + 20 Hz snapshots — proven by the agent in play mode through Unity MCP.
**Exit condition:** An MCP play-mode run reaches `InWorld` with console + screenshot evidence, and the pre-MCP editor workarounds are gone.

### [ ] T6.1 — Verify join flow via MCP; remove editor-control workarounds

**Depends on:** T4.2 (a running server is needed; the dockerized server from T5.3 works)
**Files:** `dumb-unity-client/Assets/Scripts/Editor/JoinUiBuilder.cs` (delete), `dumb-unity-client/Assets/Scripts/Editor/Demo.Editor.asmdef` (delete — folder is empty afterwards), `dumb-unity-client/Assets/Scripts/Dev/ScreenshotKey.cs` (delete), existing `Protocol.cs` / `NetworkClient.cs` / `JoinScreen.cs` (change only if verification fails), `dumb-unity-client/Assets/Scenes/SampleScene.unity` (already wired)

**Context:** Code from the original T6.1 already exists: `Protocol.cs`, `NetworkClient.cs` (FSM + main-thread events), `JoinScreen.cs`, `Tests/EditMode/ProtocolTests.cs`. SampleScene already holds `EventSystem`, `JoinCanvas` and `DemoNet` (`NetworkClient` + `JoinScreen`), built by the `Demo/Build Join UI` menu before MCP existed. `ScreenshotKey` (F12) also predates MCP. MCP replaces both. The join flow has not been verified end-to-end yet.

**Do:**
- Delete the three workaround files (with `.meta`) and the now-empty `Scripts/Editor/` and `Scripts/Dev/` folders. `refresh_unity`; console clean.
- `run_tests` EditMode, assembly `Demo.Tests.EditMode` → `ProtocolTests` green.
- Happy path (server up): `T6.1-before.png` (edit mode) → `manage_editor play` → `execute_code`: find `NameField` / `JoinButton` GameObjects, set `InputField.text = "McpTester"`, invoke `Button.onClick` → after ~1 s `read_console filter_text "[net]"` shows `joined playerId=…` then `snapshot players=…` lines → `T6.1-after.png` (panel hidden, status `InWorld`) → stop.
- Bad name: play, name `"   "`, invoke Join → console `[net] error bad_name`; status text reads `Error: …` (read it via `execute_code`).
- Server loss: play, join, `docker compose -f dumb-server/docker/docker-compose.yml stop server` → console `[net] disconnected:` warning, state `Disconnected`, no exceptions. Restart the server.
- Confirm the overlay join UI shows in MCP play-mode screenshots (first real check — see Open questions).

**Acceptance:**
- [ ] Workaround files gone; project compiles; no new `Demo*` errors/warnings in the console.
- [ ] `Demo.Tests.EditMode` passes via `run_tests`.
- [ ] Console shows `joined`, then snapshot lines at ~20/s (count lines over a timed window); status `InWorld`.
- [ ] Blank name → `bad_name` surfaced in the status line.
- [ ] Stopping the server → `Disconnected`, no hang or unhandled exception.
- [ ] `docs/screenshots/T6.1-before.png` (join screen) and `T6.1-after.png` (`InWorld`).

---

## Phase 7 — Unity world rendering

**Goal:** The editor renders the local player and remote players from the glb model with name labels, moves the local player with WASD, interpolates server snapshots, and follows with the camera. All assets are authored and verified through MCP.
**Exit condition:** In MCP play mode, the local avatar walks under synthetic WASD and bot-driven remote avatars move smoothly; screenshots captured.

### [x] T7.1 — Demo.unity scene + CameraFollow

**Depends on:** T6.1
**Files:** `dumb-unity-client/Assets/Scripts/Game/CameraFollow.cs` (new), `dumb-unity-client/Assets/Scenes/SampleScene.unity` → `Demo.unity` (renamed via MCP), `dumb-unity-client/Assets/Materials/Ground.mat` (new, MCP), `dumb-unity-client/ProjectSettings/EditorBuildSettings.asset` (via `manage_build action=scenes`)

**Context:** SampleScene is already wired and verified in T6.1. Renaming it keeps that wiring and its GUID; no second scene to keep in sync.

**Do:**
- `CameraFollow.cs` — MonoBehaviour: public `target` Transform (set by `ZoneView` in T7.3); `LateUpdate` moves toward `target.position + offset` (default `(0, 25, -18)`) with damped `Lerp` and looks at the target. No target → do nothing.
- Via MCP: `T7.1-before.png`; `manage_asset rename` SampleScene → `Demo`; add a Plane named `Ground` at origin, scale `(10,1,10)` (100×100 m) with `Ground.mat` (URP Lit, neutral gray, via `manage_material`); add `CameraFollow` to Main Camera; `manage_scene save`; `manage_build action=scenes` → only `Assets/Scenes/Demo.unity`.
- Verify: play, join via `execute_code` (as T6.1) → camera sees the gray ground, join flow works, console clean → `T7.1-after.png` → stop.

**Acceptance:**
- [x] `manage_build action=scenes` lists only `Assets/Scenes/Demo.unity`.
- [x] Join via MCP works in Demo; no new console errors.
- [x] `T7.1-before.png` / `T7.1-after.png` captured (camera has no follow target until T7.3).

### [x] T7.2 — PlayerAvatar AnimatorController + prefab + name label

**Depends on:** T7.1
**Files:** `dumb-unity-client/Assets/Scripts/Game/PlayerAvatar.cs` (new), `dumb-unity-client/Assets/Animations/PlayerAvatar.controller` (new, MCP), `dumb-unity-client/Assets/Prefabs/PlayerAvatar.prefab` (new, MCP)

**Context:** Clips were read through MCP: `female-run-idle-model.glb` contains `Idle_A` (3.125 s, looping) and `Run_Female` (0.833 s, looping). The imported model has an `Animator` with **no controller and no avatar** (generic rig). No discovery step needed.

**Do:**
- Controller via `execute_code` (`AnimatorController.CreateAnimatorControllerAtPath`): bool parameter `Moving`; state `Idle` (clip `Idle_A`, default) and `Run` (clip `Run_Female`); `Idle→Run` when `Moving`, `Run→Idle` when `!Moving`; no exit time; ~0.15 s transition. Clips come from `AssetDatabase.LoadAllAssetsAtPath` on the glb.
- Check TextMeshPro is available (`execute_code` type lookup for `TMPro.TextMeshPro`; expected to ship inside uGUI 2.6). If missing, use legacy `TextMesh` — no new package.
- `PlayerAvatar.cs` — `[SerializeField] Animator animator`, `[SerializeField] TMP_Text label`. `SetState(string state)` → `animator.SetBool(MovingHash, state == "walk")`. `SetName(string)`. `SetLocal(bool)` tints the label. `LateUpdate` turns the label to face `Camera.main`.
- Prefab via MCP: instantiate the glb in the scene, assign the controller to its `Animator`, add `PlayerAvatar`, add child `Label` (`TextMeshPro`) just above the `head` bone (measure its world height with `execute_code`), wire refs with `manage_components set_property`, `manage_prefabs create_from_gameobject` → `Assets/Prefabs/PlayerAvatar.prefab`, delete the scene instance, save.
- Verify: `T7.2-before.png`; place a temporary prefab instance in view; play; `execute_code` `SetName("Test")`, `SetState("idle")` → assert `GetCurrentAnimatorStateInfo(0).IsName("Idle")`; `SetState("walk")`, wait past the blend → assert `IsName("Run")` → `T7.2-after.png`; stop; delete the instance; save.

**Acceptance:**
- [x] `idle` → animator state `Idle`; `walk` → `Run` (asserted via `execute_code`, not eyeballed).
- [x] Label renders above the head, faces the camera, readable at follow distance.
- [x] Prefab instantiates with no missing-reference warnings.
- [x] `T7.2-before.png` / `T7.2-after.png` (avatar running with label).

### [x] T7.3 — ZoneView: spawn players, apply snapshots, interpolate

**Depends on:** T7.2
**Files:** `dumb-unity-client/Assets/Scripts/Game/ZoneView.cs` (new), `Demo.unity` (MCP wiring)

**Context:** `NetworkClient` raises `OnJoined` / `OnSnapshot` / `OnDisconnected`; the prefab exists. The client reconciles purely from snapshots (`playerJoined`/`playerLeft` may be ignored; the server still emits them).

**Do:**
- `ZoneView.cs` — `[SerializeField] NetworkClient client`, `PlayerAvatar avatarPrefab`, `CameraFollow cameraFollow`; `Dictionary<long, PlayerAvatar>`. `OnJoined` stores `localPlayerId`. `OnSnapshot`: spawn unknown ids (`SetName`, `SetLocal` for the local id, which also becomes `cameraFollow.target`); store target position `(x,0,z)`, target rotation `Quaternion.LookRotation(new Vector3(cos(yaw), 0, sin(yaw)))` (server yaw is `atan2(dir_z, dir_x)`; model forward is +z) and state; destroy avatars whose ids are missing from the snapshot. `OnDisconnected`: destroy all, clear.
- `Update`: move each avatar toward its target (`Vector3.Lerp` with an exponential factor), `Quaternion.Slerp` toward target rotation, `SetState`. Smooth the 20 Hz steps; never snap except on spawn.
- Wire via MCP: add `ZoneView` to `DemoNet`, set its three references, save.
- Verify with bots as remote players: from `dumb-server/`, `cargo run --release -p bot -- --clients 5 --duration 60`; play; join as `McpTester`; after 2 s `execute_code`: avatar count, local flag, `cameraFollow.target`; sample one bot avatar's position on consecutive frames; `T7.3-after.png`. Let the bots finish; after the 5 s grace, count again.

**Acceptance:**
- [x] 5 bots + local → 6 avatars, labels match names, local avatar marked and followed by the camera.
- [x] A bot avatar's position changes every rendered frame, not only every 50 ms (interpolated, no stepping).
- [x] Bots exit → their avatars are removed after the grace period (count back to 1).
- [x] `T7.3-before.png` / `T7.3-after.png` (local avatar + bot avatars with labels).

### [x] T7.4 — MovementInput: WASD to input messages

**Depends on:** T7.3
**Files:** `dumb-unity-client/Assets/Scripts/Input/MovementInput.cs` (new), `Demo.unity` (MCP wiring)

**Context:** Input System 1.20 is installed. Poll `Keyboard.current` each frame; no `PlayerInput` component. The agent tests through the real input path with synthetic Input System keyboard events, so `MovementInput` itself is under test.

**Do:**
- `MovementInput.cs` — `[SerializeField] NetworkClient client`. In `Update`, only when `client.CurrentState == InWorld`: build a direction from W(+z) S(−z) D(+x) A(−x), normalize, call `client.SetInput(vx, vz)` every 100 ms and immediately when the direction changes (including release → `(0,0)`).
- Wire via MCP: add to `DemoNet`, set `client`, save.
- Verify: play, join; `execute_code` read local avatar position → `InputSystem.QueueStateEvent(Keyboard.current, new KeyboardState(Key.W))` → next call ~1 s later reads position → `new KeyboardState(Key.D)` → ~1 s → read → `new KeyboardState()` (release) → ~0.5 s → read animator state. `T7.4-after.png` taken mid-walk.

**Acceptance:**
- [x] Holding W for ~1 s raises local `z` by roughly `SPEED` (5 m/s) × held time; holding D raises `x`.
- [x] Releasing keys → server state `idle`, local animator back to `Idle`.
- [x] `T7.4-before.png` / `T7.4-after.png`.

---

## Phase 8 — Acceptance + demo polish

**Goal:** Two rendered clients prove the two-client AC; the full 150-player run (1 MCP-driven Unity client + 149 bots) is recorded; the demo is presentable and starts with one command.
**Exit condition:** `docs/demo-run.md` final entry shows 150/150 with p95 < 100 ms and a clean two-client check, with screenshots.

### [x] T8.1 — Launch-arg auto-join + standalone Windows build via MCP

**Depends on:** T7.4
**Files:** `dumb-unity-client/Assets/Scripts/UI/JoinScreen.cs` (launch args), `dumb-unity-client/Assets/Tests/EditMode/JoinScreenArgsTests.cs` (new), `dumb-unity-client/Build/` (output)

**Context:** MCP drives only the editor. The standalone player is the second rendered client and nobody types into it, so it joins from launch args.

**Do:**
- `JoinScreen` — `public static string ArgValue(string[] args, string key)` returns the value after `key`, or null. In `Start`: `-host <url>` overrides the host field; `-name <name>` fills the name field and calls `OnJoinClicked()`. No args → unchanged behavior.
- `JoinScreenArgsTests` — key present, key absent, key as last arg with no value.
- `manage_build action=build target=windows64 development=true output_path=Build/dumb-client.exe`; poll `status` until done.
- Launch: `Start-Process dumb-unity-client\Build\dumb-client.exe -ArgumentList '-screen-fullscreen 0 -screen-width 1280 -screen-height 720 -name BuildB'`. Read `Player.log` under `%USERPROFILE%\AppData\LocalLow\<company>\<product>\` (names via `manage_build action=settings`) for `[net] joined`.
- Editor joins as `EditorA` via MCP; `execute_code` confirms an avatar labeled `BuildB` with a different id.

**Acceptance:**
- [x] `JoinScreenArgsTests` pass via `run_tests`.
- [x] Build succeeds via `manage_build`; exe exists.
- [x] Build auto-joins (`Player.log` shows `joined`); editor shows the `BuildB` avatar; player ids differ.

### [x] T8.2 — Two-rendered-client acceptance (A moves, B sees it)

**Depends on:** T8.1
**Files:** verification only (no code changes expected); results feed `docs/demo-run.md` in T8.3

**Context:** Requirement: "A moves → B sees it at 20 Hz with end-to-end latency < 100 ms." A = editor (MCP-driven), B = standalone build (auto-joined observer). B is outside MCP.

**Do:**
- Server up with DB. Launch B windowed with `-name BuildB`. Editor A joins via MCP.
- Capture B's window before A moves: PowerShell `PrintWindow` with `PW_RENDERFULLCONTENT` (captures even when occluded; sends no input) → `docs/screenshots/T8.2-B-before.png`.
- A holds W ~3 s via synthetic keyboard events. Capture B again → `T8.2-B-after.png`; MCP screenshot of A → `T8.2-A-after.png`.
- Numeric cross-check while A walks: `cargo run --release -p bot -- --clients 1 --duration 20 --json check.json`; p95 includes samples from A's and B's `t0` echoes.

**Acceptance:**
- [x] B's screenshots show A's avatar at a different position after A walks.
- [x] Bot report p95 < 100 ms.
- [x] Both clients' `playerId`s present and distinct in snapshots.

### [x] T8.3 — Full 150-player acceptance (1 Unity client + 149 bots) + demo-run.md

**Depends on:** T8.2
**Files:** `docs/demo-run.md` (final entry)

**Context:** Requirements AC: 150 players in one zone, all-to-all 20 Hz snapshots, p95 < 100 ms, within the machine's budget, plus "1 human + 149 bots shows multi-unit gameplay". The Unity client stands in for the human and is driven through MCP.

**Do:**
- Stack up. Editor joins via MCP; `cargo run --release -p bot -- --clients 149 --duration 120 --json final-150.json`. Walk the editor avatar with synthetic WASD; MCP screenshots of the crowd; `manage_profiler` or `execute_code` frame-time sample for client FPS under 150 avatars.
- Write the final entry in `docs/demo-run.md`: machine specs and date; commands; 150/150 connected, zero forced drops, per-client recv rate, latency avg + p95, `/metrics` cpu%/mem at peak; client frame time; the T8.2 two-client check; screenshot paths; caveats. Every requirement acceptance criterion gets a named verification row.

**Acceptance:**
- [x] 150/150 connected (1 Unity + 149 bots) with zero drops over the run.
- [x] Per-client recv ≈ 19–20 Hz; p95 < 100 ms.
- [x] `docs/demo-run.md` final entry complete with specs, commands, results, screenshots, and caveats.
- [x] Crowd screenshots from the Unity client captured.

### [x] T8.4 — Demo polish + one-command bring-up

**Depends on:** T8.3
**Files:** `docs/demo-run.md` (add "Quick start"), optional Unity tweaks (`PlayerAvatar` label sizing, `ZoneView` local-player emphasis, `CameraFollow` offset)

**Context:** Functional and proven. Make it presentable for stakeholders and verify the one-command startup the requirements promise.

**Do:**
- Polish via MCP with before/after screenshots: labels readable over a full crowd (size or distance scaling), subtle local-player highlight, camera framing that shows the crowd.
- Add "### Quick start" to `docs/demo-run.md`: `docker compose -f dumb-server/docker/docker-compose.yml up --build -d`; open `Assets/Scenes/Demo.unity`, Play, Join (or run `Build\dumb-client.exe -name <name>`); `bot --clients 149`; expected numbers and where the evidence lives.
- Verify from a clean slate: `docker compose up --build -d` alone brings up MySQL + server; editor joins with the default host field (MCP); bots join.

**Acceptance:**
- [x] From clean state, `docker compose -f dumb-server/docker/docker-compose.yml up --build -d` yields a joinable server with no manual steps.
- [x] Labels readable across the crowd and local player visually distinct (`T8.4-before.png` / `T8.4-after.png`).
- [x] "Quick start" section present and accurate.

---

## Open questions

- **Synthetic keyboard in play mode.** Queued Input System keyboard events may be ignored when the Game view lacks focus (Input System "Play Mode Input Behavior" setting). Verify first in T7.4; fallback is changing that project setting so device input always goes to the Game view.
- **Overlay UI in MCP screenshots.** The edit-mode capture showed no UI; play-mode capture without a `camera` argument should include Screen Space Overlay canvases per the tool docs. Verify in T6.1; if it doesn't, UI state is proven from console lines and `execute_code` reads instead.
- **Unfocused editor barely ticks in play mode** (found in T6.1: 4 frames in ~40 s with `Application.runInBackground = false`). Network messages queue in the background task and drain in a burst once frames resume, so rates measured while stalled are wrong. **Resolved:** Player Settings → Run In Background is on (`ProjectSettings.asset` `runInBackground: 1`). Verified: unfocused editor play mode runs ~680 fps with no runtime toggle. Also keeps the unfocused standalone client B ticking in T8.2.
- **MCP calls across domain reloads.** Entering play mode or recompiling can drop an in-flight call. Always wait for `mcpforunity://editor/state` `data.advice.ready_for_tools` before the next call.
- **Standalone window capture (T8.2).** `PrintWindow` with `PW_RENDERFULLCONTENT` is expected to capture a DirectX window while occluded; unverified here. Fallback: the user takes B's screenshot.
- **Rendering 150 animated models** may drop the editor's frame rate. Networking acceptance (p95) is server-side and unaffected; frame time is recorded in T8.3; mitigation (no shadow casts for remote avatars) only if polish needs it.
- **`t0`-echo latency is single-machine-valid** (shared clock). Cross-machine runs treat it as relative; recorded as a caveat in `demo-run.md`.
- **Client use of `playerJoined`/`playerLeft` events**: the server emits them; the client reconciles from snapshots only (YAGNI). Revisit if the client ever needs early spawn hints.
- **Duplicate-name + profile sharing** is last-writer-wins on the shared profile row. Accepted edge case.
