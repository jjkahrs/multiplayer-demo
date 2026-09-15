# Technical Design: 150-Player Zone Sync Demo

## Overview
Builds the rapid-prototype multiplayer demo specified in [REQUIREMENTS.md](./REQUIREMENTS.md): one Rust WebSocket server authoritative over player movement in a single zone, broadcasting 20 Hz position snapshots to all connected clients, with a Unity client that renders the world and a Rust CLI bot loader that proves 150 concurrent players hold under 100 ms p95 end-to-end latency. Backing store is MySQL via docker-compose.

Contains three runtime pieces: the **server** (Rust + axum + sqlx), the **bot loader** (Rust CLI that simulates N fake players), and the **Unity client** (renders local + remote players with the `female-run-idle-model.glb`).

## Context & constraints
- Server project `./dumb-server` is **empty** (greenfield). Client `./dumb-unity-client` is a stock Unity 6000.6.0f1 template (URP 17.6, Input System 1.20, uGUI 2.6, gltfast 6.20, no networking package).
- Model `Assets/Models/female-run-idle-model.glb` imports as a generic rig with an `Animator` but **no controller and no avatar**. Clips (read through Unity MCP): `Idle_A` (3.125 s, looping) and `Run_Female` (0.833 s, looping).
- **Unity MCP** (`com.coplaydev.unity-mcp`, HTTP transport on port **8077**, clear of the game server's 8080) gives the agent editor control: scene/prefab/component edits, `execute_code`, play mode, console, screenshots, EditMode test runs, player builds. Dev tooling only — nothing in the shipped client depends on it.
- Decided stack: **axum + tokio** (HTTP + WS in one app), **sqlx** (runtime-checked queries — no DB needed at compile time), JSON over WS, 20 Hz server-authoritative tick.
- Latency metric: **t0 echo** — valid because the acceptance run happens on one machine (sender and receiver share a clock).
- AGENTS.md: YAGNI, Clean Code, loose coupling, event-driven, finite state machines; no git add/commit/push by us.
- Out of scope (per requirements): chat, combat, interest management, multiple zones, anti-cheat, reconnect/resume (socket loss = new session; only the DB position is reused), gamesite-level physics.

## Architecture

### Runtime data flow

```
                            ┌────────────────────────────── Server (dumb-server) ─────────────────────────────┐
 Unity client / bot ──ws──▶ │  axum /ws  ─▶  conn task ─▶  [ClientEvent channel] ─▶  Zone task (single authority) │
                            │                 read task        │                        │  20 Hz tick               │
                            │                     └────────────┘   Join/Input/Close  ──▶ │  sim + persist             │
                            │  /health /metrics ◀── metrics ◀─────────────────────────────┘  │                     │
                            │  broadcast channel ◀─── Arc<[u8]> snapshot (pre-serialized) ──┘                     │
                            └─────┴──── ws ◀── write tasks (one per connection, non-blocking) ────────────────────┘
                                  MySQL ◀── profile load/save (sqlx pool)
```

Key design choice: **world state lives only inside the Zone task** — no locks, no shared mutable state for simulation. Reads and writes funnel through one async task, which makes the 150-player target predictable and the code trivially correct. Everything else is event-driven around it.

### Server modules (`dumb-server/crates/server/src/`)
Each module is a Rust file with a single responsibility:

| Module | Responsibility |
|---|---|
| `main.rs` | Config, sqlx pool, spawn Zone task, launch axum. |
| `http.rs` | Routes: `GET /ws` (upgrade), `GET /health`, `GET /metrics`. Same port (8080). |
| `ingest.rs` | Per-connection read loop: parse + validate raw WS → `ClientEvent` → channel to Zone. Owns the socket-closed detection. |
| `writer.rs` | Per-connection write loop: subscribe to snapshot broadcast, push frames. Never blocks the Zone. |
| `zone.rs` | Owns `World`; consumes `ClientEvent`s; 20 Hz tick (simulate → serialize once → broadcast); persists on player removal. |
| `player.rs` | `Player` struct + connection/player **FSM** (see below). |
| `profile.rs` | sqlx repository: `load_or_create(name) → (profile_id, spawn_pos)`, `save(profile_id, x, z, yaw)`. |
| `metrics.rs` | `Arc<Mutex<Stats>>` updated by Zone per tick; read by `/metrics`. |
| `config.rs` | Env-config: `BIND` (default `0.0.0.0:8080`), `DATABASE_URL`, `TICK_HZ` (20), `GRACE_MS` (5000), `SPEED` (5.0 m/s), `WORLD_HALF` (50). |

### Bot loader (`dumb-server/crates/bot/`)
A separate crate so its dependencies (`tokio-tungstenite`, `clap`, `csv`/TUI) don't bloat the server. Shares JSON types + validation with the server through the `protocol` crate.

### Unity client (`dumb-unity-client/Assets/`)
```
Scripts/
  Protocol/Protocol.cs          # [Serializable] message classes + JsonUtility (camelCase fields)       (exists)
  Networking/NetworkClient.cs   # ClientWebSocket driver; FSM Disconnected→Connecting→Joining→InWorld  (exists)
  UI/JoinScreen.cs              # uGUI join panel + status line; -host/-name launch args auto-join     (exists; args in T8.1)
  Game/ZoneView.cs              # owns player GameObject registry; applies snapshots
  Game/PlayerAvatar.cs          # model instance + Animator + name label (TMPro world-space)
  Game/CameraFollow.cs          # simple follow of local player
  Input/MovementInput.cs        # polls Input System keyboard → SetInput at 10 Hz + on change
Animations/PlayerAvatar.controller  # Idle_A ⇄ Run_Female on bool parameter Moving
Prefabs/PlayerAvatar.prefab
Scenes/Demo.unity               # SampleScene renamed: ground, light, camera, EventSystem, JoinCanvas, DemoNet
Tests/EditMode/                 # Demo.Tests.EditMode (exists: ProtocolTests)
```
Scene, prefab and controller are **authored by the agent through Unity MCP** and saved as ordinary assets — no editor-builder scripts. The pre-MCP workarounds are deleted: `Scripts/Editor/JoinUiBuilder.cs` (+ `Demo.Editor.asmdef`) and `Scripts/Dev/ScreenshotKey.cs`.

`-host` / `-name` launch args exist so the standalone build (second rendered client, outside MCP's reach) joins without anyone typing into it.

### Editor automation & verification loop (Unity MCP)
Every Unity task is built and proven by the agent in the running editor. No OS mouse/keyboard: all driving happens in-engine through MCP.

| Step | MCP call |
|---|---|
| 1. Compile | write C# → `refresh_unity`; wait for `mcpforunity://editor/state` `data.advice.ready_for_tools` |
| 2. Clean console | `read_console` error + warning: no new entries from `Demo*` assemblies. Known noise: `generators.ai.unity.com` `NoSubscription` (AI Assistant package). |
| 3. Unit tests | `run_tests` EditMode, assembly `Demo.Tests.EditMode`; poll `get_test_job` |
| 4. Before screenshot | `manage_camera screenshot`, `output_folder: "Captures"`, no `camera` arg (ScreenCapture path keeps overlay UI) |
| 5. Author assets | `manage_scene` / `manage_gameobject` / `manage_components` / `manage_prefabs` / `manage_material`; `execute_code` where no dedicated tool fits (AnimatorController); `manage_scene save` |
| 6. Play + drive | `manage_editor play`; wait for `play_mode.is_changing = false`; `execute_code` sets `InputField.text`, invokes `Button.onClick`, queues Input System keyboard state for WASD |
| 7. Observe | `read_console filter_text "[net]"`; `execute_code` reads avatar transforms / animator state |
| 8. After screenshot | as step 4 |
| 9. Stop | `manage_editor stop` |

- **Screenshots:** MCP writes only inside the Unity project, so capture to `dumb-unity-client/Captures/<task>-<before|after>.png` (outside `Assets/`, never imported) and copy to `docs/screenshots/`.
- **Drive the real input path.** Movement tests queue synthetic Input System keyboard events so `MovementInput` is exercised. Calling `NetworkClient.SetInput` directly would skip the code under test, and `MovementInput` would overwrite it with `(0,0)` next tick.
- **Test scope:** EditMode tests + MCP play-mode checks. No PlayMode test assembly (YAGNI).

### Finite state machines
Server `PlayerState` (per player, inside the Zone):
```
Joining ──validate+load──▶ Active ──socket closed──▶ Suspended(deadline) ── ‥5 s ‥──▶ Removed
                                        └──────────── sentinels: position saved on ANY exit ─────────┘
```
Socket close → `Suspended` (frozen in place, still broadcast so others see it idle) → persisted immediately, removed from world + `playerLeft` emitted when the 5 s grace expires. No resume.

Client `NetworkClient` states drive which UI is visible and whether input is sent — a clear FSM stops the client sending input before it has a server playerId.

## Data models & interfaces

### WebSocket protocol (JSON, camelCase — matches Unity `JsonUtility`)

Client → Server:
```json
{ "type": "join",  "name": "Bob" }
{ "type": "input", "vx": 0.0, "vz": 1.0, "seq": 42, "t0": 912345 }   // vx/vz ∈ [-1,1], normalized; seq monotonic; t0 = local ms
{ "type": "leave" }
```
Server → Client:
```json
{ "type": "joined", "playerId": 7, "name": "Bob", "x": 0.0, "z": 0.0, "yaw": 1.5708, "speed": 5.0, "worldHalf": 50.0, "tickHz": 20 }
{ "type": "snapshot", "tick": 1234, "players": [ { "id": 7, "name": "Bob", "x": 12.0, "z": -3.5, "yaw": 2.0, "state": "walk", "seq": 42, "t0": 912345, "ageMs": 150 } ] }
{ "type": "playerJoined", "id": 9, "name": "Alice" }
{ "type": "playerLeft", "id": 9 }
{ "type": "error", "code": "bad_name", "message": "..." }
```
Decisions:
- **`t0` echo**: each snapshot entry carries the newest `seq`/`t0` that player sent. A receiver computes `localNow - t0` = **one-way A→server→B latency**. Valid only with a shared clock (single demo machine); cross-machine runs must use it as a relative number. The bot loader reports exactly this for all cross-peer entries.
- **`yaw` in radians**, computed from the movement direction when walking; frozen while idle. Unity applies it as `Quaternion.AngleAxis(yaw, Vector3.up)`.
- **Prediction fields** (`speed`/`worldHalf`/`tickHz` in `joined`, `tick` in `snapshot`, `ageMs` per player): see [TECHNICAL_DESIGN-prediction.md](./TECHNICAL_DESIGN-prediction.md). `ageMs` is how long the server has integrated the player's current direction.
- **`playerId` is session-unique, not the DB row id.** Two concurrent "Bob"s get different playerIds (distinguishable) while sharing one profile row.

### MySQL
Schema lives in `dumb-server/docker/init.sql` (single source of truth, mounted as an init script — no compile-time migrations; sqlx is used runtime-checked so the build never needs a live DB):
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
- Join: `load_or_create(name)` → row with the most recent `updated_at`; adopt its position, else spawn at origin/near other players.
- Leave (immediate on socket close, not after grace): `save(id, x, z, yaw)`. Two same-name players → last writer wins (decided; documented edge case).

### Bot loader CLI
```
bot --server ws://127.0.0.1:8080/ws --clients 150 --duration 60 --move random
```
Each bot: connects → `join("Bot-<i>")` → random-walk `input`s (pick a direction + speed every 1–3 s, turn inside bounds) → on each snapshot measures `now - t0` against **other** players' entries (true A→B one-way) → aggregates. Final report: connections ok/total, snapshots/s per client (avg), one-way latency avg + p95, server `/metrics` CPU/RAM.

## Implementation plan
Ordered; each step ends runnable. Depends between steps noted.

1. **Workspace + protocol crate.** `dumb-server/Cargo.toml` (workspace: `protocol`, `server`, `bot`). `protocol` defines all message structs + `serde` and `validate_name()`. Unit tests for serialization round-trips and name validation (length, control chars, trim). *No deps on other steps.*
2. **Server skeleton.** axum app on 8080: `/health` → 200 JSON, `/metrics` → JSON stub, `/ws` accepting connections (echo raw frame). Config module. *Depends: 1.*
3. **Zone core (no DB).** `ClientEvent` channel, `World`, 20 Hz tick task, broadcast channel of pre-serialized `Arc<[u8]>` snapshots, per-connection write tasks. Join/input/leave handled in memory; `playerId` counter; world bounds clamp (±50 m); FSM `Joining→Active→Suspended→Removed`. Integration test with a `protocol`-based tokio ws client: join, send input, assert own position advances in snapshots, leave.
4. **Persistence.** `docker/docker-compose.yml` (MySQL 8.4 + server) + `docker/init.sql`; sqlx pool; `profile.rs` load/save wired into join and removal; rejoin restores position. *Depends: 3.*
5. **Grace + events + name rules.** Suspended deadline → remove + `playerLeft` + save; `playerJoined`/`playerLeft` emission; duplicate-name handling (distinct playerIds); invalid input ignored (out-of-range magnitude, unknown types). *Depends: 3–4.*
6. **Bot loader.** `bot` crate: connect/join/random-walk/measure/report. Run 150 bots against local server; record into `docs/demo-run.md`. Add server-side `sysinfo` cpu/ram into `/metrics`. *Depends: 5.*
7. **Unity networking.** `Protocol.cs`, `NetworkClient.cs` (ClientWebSocket + FSM), `JoinScreen.cs` — written. Remaining: delete the pre-MCP workarounds, then verify through the MCP loop: join → `joined` + snapshots in console, bad name surfaced, server stop → `Disconnected`. *Needs the running server from step 6.*
8. **Unity world.** Through MCP: SampleScene renamed to `Demo.unity` + ground + `CameraFollow`; `PlayerAvatar.controller` (`Idle_A` ⇄ `Run_Female` on `Moving`); `PlayerAvatar` prefab (model + TMPro label); `ZoneView` (spawn players, apply snapshots, lerp toward targets, drive `Moving` from `state`); `MovementInput`. Verified in play mode with a few bots as remote players and synthetic WASD for the local player.
9. **Standalone player build + acceptance.** `-host`/`-name` launch args; Windows build via `manage_build`; MCP-driven editor (A) + auto-joined build (B) as the two rendered clients. Full acceptance: editor client + 149 bots, record metrics, confirm p95 < 100 ms, write `docs/demo-run.md`. *Depends: 8.*
10. **Demo polish.** Name labels readable over 150 players, camera feel, in-window player/local-player emphasis, one-command bring-up docs (`docker compose up`).

## Testing & verification
Mapped to REQUIREMENTS acceptance criteria (AC):

| AC (from REQUIREMENTS.md) | Verification |
|---|---|
| Single client joins with valid name, gets playerId + position | `protocol` integration test + MCP play-mode check: `[net] joined` in console, status → InWorld, screenshot. |
| Rejoin restores last saved position | Integration test: join "Bob", move, disconnect, wait grace, rejoin "Bob" → snapshot starts at saved x/z. |
| Two clients: A moves, B sees it < 100 ms | Step 9: editor A walks via MCP synthetic input; auto-joined build B captured before/after with `PrintWindow`; bot report p95. |
| 150 accepted, full snapshots, p95 < 100 ms, CPU/RAM within budget | Step 6/10 bot run: report 150/150, per-client recv ≈ 20 Hz, p95 lines, `/metrics` recorded in `demo-run.md`. |
| Disconnect → grace → removed + persisted + `playerLeft` | Integration test asserts timeline (in snapshots during grace, gone after, DB row updated). |
| 1 human + 149 bots shows multi-unit gameplay | Step 9 run: MCP-driven editor client + 149 bots, MCP crowd screenshots + client frame time. |
| Duplicate names both accepted, distinct playerId | Integration test: two joins "Bob" → different ids, both in snapshots. |
| Invalid input ignored | Unit/integration: |v|>1, junk types, 0-length/control-char names → `error` or ignored, no panic. |

Edge cases with dedicated tests: name trimmed/empty/17 chars/control chars; input burst spam (no crash, capped rate); snapshot lag (slow bot sees newer frames, no stall); 150 connections opened simultaneously; server restart mid-world (last persisted position is last save).

Before/after screenshot for each Unity change, captured through MCP (see *Editor automation & verification loop*).

## Open questions / risks
- **Synthetic keyboard in play mode.** Queued Input System keyboard events may be ignored while the Game view lacks focus (Input System "Play Mode Input Behavior" setting). Verified first in T7.4; fallback is changing that project setting.
- **Overlay UI in MCP screenshots.** Edit-mode capture showed no UI; play-mode capture without a `camera` arg should include overlay canvases per the tool docs. Verified in T6.1; fallback is proving UI state from console + `execute_code` reads.
- **MCP calls across domain reloads.** Play-mode entry or recompiles can drop an in-flight call; wait for `ready_for_tools` before each step.
- **Standalone build is outside MCP.** Its evidence comes from `Player.log` and a `PrintWindow` capture (no input sent). Occluded-window capture is unverified; fallback is a user-taken screenshot.
- **Rendering 150 animated models** may drop the demo machine's frame rate even though networking stays under budget. Mitigation: camera is above the crowd, models are a single mesh; if it's still heavy, enable occlusion culling / drop shadow casts for bots. Networking acceptance (p95) is server-side and unaffected.
- **`t0`-echo validity is single-machine only.** Documented in the protocol section; cross-machine runs treat it as relative.
- **docker container CPU/RAM** from `sysinfo` reports host-level numbers for the container; record RAM + server-process CPU with a caveat in `demo-run.md`.