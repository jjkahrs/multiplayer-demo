# Requirements: Multiplayer Demo — 150-Player Zone Sync

## Goal
Validate the core MMO communication bet: a single server can hold **150 concurrent players in one zone** and stream **server-authoritative position updates in real time** to every client, presented as a polished-enough demo for external stakeholders. This is a rapid prototype to prove the networking path (client ↔ websocket ↔ Rust server ↔ MySQL), not a full game.

## Scope

**In scope:**
- One zone containing all connected players; every player's position is visible to every other player.
- Server-authoritative movement: clients send movement input, server simulates positions, clients render server state.
- Player identity: each player has a display name shown above their character.
- 20 Hz position snapshots broadcast from server to all connected clients.
- 3D flat-plane world, WASD movement, humanoid characters with idle + walk animations and name labels.
- Character models sourced from `./dumb-unity-client/Assets/Models/female-run-idle-model.glb`.
- Connect flow: enter a display name → Join → enter world. No passwords.
- MySQL persists player profile data (name, last position), loaded on join and saved on disconnect.
- docker-compose for one-command local server setup (MySQL + server, default ports).
- Headless Rust bot loader that connects N fake clients, moves them, and reports latency/throughput so the 150-player claim is actually proven.
- Graceful disconnect: on socket loss the player is removed from the world after a short grace period.

**Out of scope:**
- Chat (global or local).
- Combat, items, loot, quests, inventory, or any gameplay beyond movement.
- Accounts/passwords, friend lists, matchmaking.
- Multiple zones, cross-zone messaging, or zone handoff.
- Interest management/distance culling (deliberate: max load proves the 150 claim).
- Game physics, collision, animations beyond idle/walk, character customization.
- Persistence of anything beyond player profile (no world state, no inventories).
- Anti-cheat, rate-limiting, reconnect/resume (socket loss = new session). Last position is restored but the session is new.

## Inputs
- **From each Unity client (JSON over websocket):**
  - `join` message: display name (validated: 1–16 chars, non-empty after trim, no control chars; duplicates allowed but distinguished by server-assigned player ID).
  - `input` message: movement intent (which of forward/back/left/right, and optionally a facing/rotation target).
  - `leave`/socket close: disconnect intent.
- **From the bot loader (Rust CLI):** identical `join`/`input` traffic, at scale (e.g. 150 bots), with deterministic (scripted or non-moving) movement patterns.
- **Trust level:** input messages are untrusted hints; server authority ignores invalid input (out-of-range magnitude, spamming) and never trusts client coordinates.

## Outputs
- **To every connected client, 20×/sec (JSON over websocket):**
  - A world snapshot: list of `{ playerId, name, position, facing, animation state }` for all currently-connected players, including the receiver (authoritative).
- **To a single client (state change events):**
  - `joined`: your player ID + your initial authoritative position (sent once on accept, restored from last saved position when available).
  - `playerJoined` / `playerLeft`: name + ID of players who join/leave while you're connected.
- **To MySQL:** on join, load profile (name, last position) or create it; on disconnect/grace expiry, write last position.
- **To operators:** bot loader prints per-run stats — connections succeeded, avg/p95 latency of last snapshot arrival, receive rate, CPU/memory on server.

## Acceptance criteria
- **Given** a running server with no players, **when** one Unity client joins with a valid name, **then** it receives its own player ID + initial position and enters the world.
- **Given** a player who previously disconnected mid-world, **when** they rejoin, **then** their last saved position is restored and used as their starting point.
- **Given** two connected Unity clients, **when** client A moves, **then** client B sees A's position update accordingly at 20 Hz with **end-to-end latency under 100 ms** (input → server → other client, measured locally).
- **Given** 150 connections from the bot loader, **when** all are active and moving, **then**:
  - the server accepts all 150 (no rejections or forced drops),
  - every client receives snapshots containing all 150 players at the expected ~20 Hz rate,
  - p95 end-to-end latency stays under 100 ms,
  - server CPU/RAM stay within the demo machine's budget (measured and reported by bot loader, recorded in `docs/demo-run.md`).
- **Given** a player disconnects (socket close), **when** the grace period expires, **then** the player is removed from snapshots, their position is persisted, and other clients are told they left.
- **Given** one human Unity client + 149 bots on the demo machine, **when** running the load test, **then** multi-unit gameplay is observed alongside the 150-connection validation.
- **Given** a duplicate display name, **when** a second player joins with the same name, **then** both are accepted and remain distinguishable in the protocol by their unique player ID (labels may show the name only).

## Constraints & dependencies
- Client: Unity 6000.6.0f1, project under `./dumb-unity-client` (currently template-only, no networking).
- Server: Rust modular monolith, MySQL backend, websocket server, under `./dumb-server` (currently empty).
- Language/format: JSON over websocket; 20 Hz snapshot loop.
- Performance target: **150 players, one zone, < 100 ms p95 end-to-end.**
- Architecture style (from AGENTS.md): YAGNI, Clean Code, loosely coupled components, event-driven design, finite state machines (player connection/zone lifecycle = FSM).
- Demo hardware: current machine's specs (used for the 150-player acceptance run; see `docs/demo-run.md`).
- Deployment/setup: docker-compose starts MySQL + server with default ports for one-command local bring-up.
- No git add/commit/push by us (AGENTS.md).
- Depends on: Unity editor license, a MySQL instance (local), a Rust toolchain, docker/compose, and hardware able to run server + Unity + bots.

## Open questions
- **OQ-1 (resolved):** Character model sourced from `./dumb-unity-client/Assets/Models/female-run-idle-model.glb`.
- **OQ-2 (resolved):** docker-compose for one-command local stack.
- **OQ-3 (resolved):** Acceptance run uses the current machine's specs.
- **OQ-4 (resolved):** Last position is persisted on disconnect and restored on join.
- **Pending:** exact bot count mix for the live stakeholder demo (running 149 bots alongside a real player vs 150 bots + observing) — decide at demo-run time.