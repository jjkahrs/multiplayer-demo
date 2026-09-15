# Demo Run: 150-Player Zone Sync

Evidence for the requirement **150 players in one zone, 20 Hz snapshots, p95 end-to-end latency < 100 ms**.

### Quick start
Prerequisites: Docker, Rust toolchain, Unity 6000.6.0f1. Run commands from the repo root.

1. **Server + MySQL (one command):**
   ```
   docker compose -f dumb-server/docker/docker-compose.yml up --build -d
   ```
   Wait until `curl.exe http://127.0.0.1:8080/health` returns `{"status":"ok"}`. The first run builds the server image and initializes MySQL from `init.sql`.
2. **Unity client:** open `dumb-unity-client` in Unity, open `Assets/Scenes/Demo.unity`, press Play, type a name, click **Join**. The host field defaults to `ws://127.0.0.1:8080/ws`. Move with WASD.
   Or run the standalone build (build it once from Unity for Windows 64-bit into `dumb-unity-client/Build/`):
   ```
   dumb-unity-client\Build\dumb-client.exe -screen-fullscreen 0 -screen-width 1280 -screen-height 720 -name <name>
   ```
   Optional `-host ws://<ip>:8080/ws`.
3. **Crowd (149 bots):**
   ```
   cargo build --release -p bot --manifest-path dumb-server/Cargo.toml
   dumb-server\target\release\bot.exe --clients 149 --duration 120
   ```
4. **Stop:** `docker compose -f dumb-server/docker/docker-compose.yml down` (add `-v` to also wipe saved profiles).

**Expected on this machine:** 149/149 bots connected, 0 drops, ≈ 20 Hz, p95 ≈ 58 ms; server ≈ 10 % of one core and < 40 MiB; Unity editor ≈ 140 fps with 150 avatars. Evidence: Run 1 and Run 2 below, raw reports [demo-run-150.json](./demo-run-150.json) and [final-150.json](./final-150.json), screenshots in [screenshots/](./screenshots/).

## Run 1 — 150 bots (T5.3)

**Date:** 2026-09-15 09:23 -04:00

### Machine
| | |
|---|---|
| OS | Windows 11 Home 10.0.26200 (build 26200) |
| CPU | Intel Core i9-14900K — 24 cores / 32 logical |
| RAM | 63.8 GB |
| Docker | 29.3.0 (VM: 32 CPUs, 31.2 GiB) |
| Rust | 1.90.0 |

Server and all 150 bots ran on this one machine.

### Build under test
No git commits in this project, so the build is identified by image:
- Server image `multiplayer-demo-server` `sha256:7a33d87a3927e81b247fbb5089ccaccf7c555b6a58686ffb1891c207d779842e` (built 2026-09-15T13:12:36Z, includes T5.2 metrics)
- MySQL `mysql:8.4`, persistence enabled (`DATABASE_URL` set)

### Commands
From `dumb-server/`:
```
docker compose -f docker/docker-compose.yml up --build -d
cargo build --release -p bot
target\release\bot.exe --clients 150 --duration 60 --json ..\docs\demo-run-150.json
```
During the run, every ~6 s: `curl.exe http://127.0.0.1:8080/metrics` and `docker stats --no-stream multiplayer-demo-server-1`.

The bot is run as a **release** build (the plan's command was `cargo run -p bot`): 150 bots each parse a 150-player snapshot 20×/s, and a debug build risks loading the bot process itself and inflating measured latency.

### Results
| Metric | Target | Result |
|---|---|---|
| Connected | 150/150 | **150/150** |
| Forced drops | 0 | **0** |
| Snapshot recv rate (avg per client) | ≈ 19–20 Hz | **20.0 Hz** |
| One-way latency avg | — | **34.9 ms** |
| One-way latency p95 | < 100 ms | **58 ms** ✅ |
| Latency samples | — | 13,313,074 |
| Server tick rate (`snapshotsPerSec`) | 20 | 19.99–20.01 throughout |
| Server tick dt | 50 ms | 49.2–50.5 ms |
| Server process CPU at peak (`/metrics` `cpuPercent`) | within budget | **9.7 %** of one core |
| Server container CPU at peak (`docker stats`) | — | 10.0 % of one core |
| Server process memory at peak (`/metrics` `memBytes`) | within budget | **29.1 MB** |
| Server container memory at peak (`docker stats`) | — | 33.8 MiB |
| Snapshot traffic sent by server | — | ≈ 61 MB/s total, ≈ 0.41 MB/s per client, ≈ 20 KB per snapshot |

Sample count sanity check: 150 bots × 149 peers × 10 inputs/s × 60 s = 13.41 M expected; 13.31 M recorded (99.3 %). The shortfall matches connect/join time at start.

Snapshot traffic is derived from `docker stats` NET I/O for the server container: 3.79 GB sent over the ~62 s from first connect to last disconnect. Approximate: it includes WebSocket framing and the small `joined`/`playerJoined`/`playerLeft` messages, and assumes Docker's decimal units.

Raw bot report: [demo-run-150.json](./demo-run-150.json).

### Server metrics during the run
| t | players | snapshots/s | tick dt ms | cpu % | mem MB | docker cpu % |
|---|---|---|---|---|---|---|
| 7 s | 150 | 20.00 | 49.8 | 9.4 | 29.1 | 9.6 |
| 14 s | 150 | 19.99 | 50.2 | 6.2 | 29.1 | 9.1 |
| 20 s | 150 | 20.00 | 50.4 | 8.6 | 29.1 | 9.1 |
| 26 s | 150 | 20.01 | 50.3 | 9.3 | 29.1 | 10.0 |
| 32 s | 150 | 19.99 | 49.7 | 9.4 | 29.1 | 9.1 |
| 38 s | 150 | 20.00 | 50.5 | 9.2 | 29.1 | 8.9 |
| 45 s | 150 | 20.00 | 49.5 | 9.1 | 29.1 | 8.8 |
| 51 s | 150 | 19.99 | 50.3 | 9.2 | 29.1 | 9.2 |
| 57 s | 150 | 20.00 | 50.0 | 9.0 | 29.1 | 8.3 |

### Caveats
- **CPU % scale:** both `cpuPercent` and `docker stats` use 100 % = one full core, so values can exceed 100 on multi-core machines. ~10 % of one core is ~0.3 % of this 32-thread machine.
- **CPU % is process-level, not host-level.** The plan expected `sysinfo` inside a container to report host-level CPU. Measured: `/metrics` `cpuPercent` (9.4 %) tracks `docker stats` container CPU (9.1 %), so it reflects the server process.
- **Latency is t0-echo on one shared clock.** Each bot stamps `t0` with Unix-epoch ms; peers compute `now − t0` when they first see a new `seq` for that player. Valid only because sender and receiver share this machine's clock; cross-machine runs must treat it as a relative number.
- **Latency includes tick wait.** A sample covers input send → server receive → wait for next 20 Hz tick (0–50 ms) → broadcast → peer parse, so ~25 ms of the average is tick alignment by design.
- **Bots share the machine with the server.** Bot CPU was not measured; bot-side parsing load is included in the latency numbers, which makes them conservative.
- **No deviation from the 100 ms target.** p95 was 58 ms.

---

## Run 2 — Final acceptance: 1 Unity client + 149 bots (T8.3)

**Date:** 2026-09-15 11:41 -04:00

### Machine
Same machine as Run 1, re-checked on the day: Windows 11 Home 10.0.26200, Intel Core i9-14900K (24 cores / 32 logical), 63.8 GB RAM, Docker 29.3.0, Rust 1.90.0. GPU for the Unity client: NVIDIA GeForce RTX 4070 Ti SUPER.

Server, MySQL, 149 bots and the Unity editor client all ran on this one machine.

### Build under test
- Server image `multiplayer-demo-server` `sha256:7a33d87a3927e81b247fbb5089ccaccf7c555b6a58686ffb1891c207d779842e` (same image as Run 1)
- MySQL `mysql:8.4`, persistence enabled
- Unity client: `Assets/Scenes/Demo.unity` in the Unity 6000.6.0f1 editor, driven through Unity MCP (no OS mouse/keyboard)

### Commands
Stack already up (`docker compose -f dumb-server/docker/docker-compose.yml up --build -d`).

1. Unity editor: Play → MCP `execute_code` sets `NameField` = `EditorA` and invokes `JoinButton`.
2. From `dumb-server/`:
   ```
   target\release\bot.exe --clients 149 --duration 120 --json ..\docs\final-150.json
   ```
3. During the run, every ~7 s: `curl.exe http://127.0.0.1:8080/metrics` and `docker stats --no-stream multiplayer-demo-server-1`.
4. Unity client under load: synthetic Input System keyboard events (W, then D, then release); MCP screenshots; frame-time sampled in-engine over 10 s.
5. After the run, with the load gone: `TEST_DATABASE_URL=mysql://demo:demo@127.0.0.1:3306/demo cargo test -p server`.

### Results
| Metric | Target | Result |
|---|---|---|
| Players in zone | 150 | **150** (1 Unity + 149 bots; `/metrics` `players` = 150 from 6 s to 118 s) |
| Bots connected | 149/149 | **149/149** |
| Forced drops | 0 | **0** (bots); Unity client stayed `InWorld` with no `[net] disconnected` or console errors |
| Snapshot recv rate (avg per bot) | ≈ 19–20 Hz | **20.0 Hz** (19.99) |
| One-way latency avg | — | **35.0 ms** |
| One-way latency p95 | < 100 ms | **58 ms** ✅ |
| Latency samples | — | 26,599,486 |
| Server tick rate (`snapshotsPerSec`) | 20 | 19.99–20.01 throughout |
| Server tick dt | 50 ms | 49.2–51.0 ms |
| Server CPU at peak (`docker stats`) | within budget | **10.1 %** of one core (`cpuPercent` peak 10.0 %) |
| Server memory at peak | within budget | **28.8 MiB** process (`memBytes`), 35.7 MiB container |
| Snapshot traffic sent by server | — | ≈ 62 MB/s total, ≈ 0.41 MB/s per client |
| Unity client avatars under load | 150 | **150** (149 bots in `Run` animation) |
| Unity client frame time (150 avatars) | — | **7.02 ms avg (142.5 fps)**, p95 8.02 ms, p99 9.04 ms, max 16.8 ms over 1,427 frames |

Sample count sanity check: 149 bots × 149 peers × 10 inputs/s × 120 s = 26.64 M expected; 26.60 M recorded (99.8 %).

Snapshot traffic is derived from `docker stats` NET I/O: 631 MB → 7.56 GB sent between the 6 s and 118 s samples (6.93 GB / 112 s). Same approximations as Run 1.

Raw bot report: [final-150.json](./final-150.json).

### Server metrics during the run
| t | players | snapshots/s | tick dt ms | cpu % | mem MiB | docker cpu % | docker mem MiB |
|---|---|---|---|---|---|---|---|
| 6 s | 150 | 20.00 | 49.9 | 9.5 | 28.5 | 9.6 | 32.9 |
| 14 s | 150 | 20.00 | 50.0 | 9.5 | 28.5 | 9.9 | 32.9 |
| 22 s | 150 | 20.00 | 49.8 | 9.5 | 28.5 | 9.6 | 35.7 |
| 29 s | 150 | 19.99 | 50.4 | 10.0 | 28.5 | 10.1 | 32.5 |
| 36 s | 150 | 20.00 | 49.8 | 9.2 | 28.5 | 9.3 | 32.9 |
| 43 s | 150 | 20.00 | 49.4 | 0.2 ⚠ | 28.5 | 9.3 | 32.7 |
| 51 s | 150 | 20.01 | 50.3 | 9.2 | 28.5 | 9.1 | 32.6 |
| 58 s | 150 | 20.01 | 50.2 | 9.1 | 28.5 | 8.8 | 32.2 |
| 65 s | 150 | 20.00 | 49.6 | 9.1 | 28.5 | 9.0 | 32.1 |
| 73 s | 150 | 20.00 | 49.2 | 0.0 ⚠ | 28.8 | 7.5 | 32.8 |
| 81 s | 150 | 20.01 | 49.4 | 7.4 | 28.8 | 7.6 | 32.4 |
| 88 s | 150 | 19.99 | 51.0 | 7.6 | 28.8 | 7.5 | 32.6 |
| 95 s | 150 | 19.99 | 50.6 | 7.7 | 28.8 | 7.8 | 33.3 |
| 103 s | 150 | 20.00 | 50.1 | 0.4 ⚠ | 28.8 | 7.3 | 32.8 |
| 111 s | 150 | 20.00 | 50.9 | 8.5 | 28.8 | 8.2 | 32.9 |
| 118 s | 150 | 20.00 | 50.7 | 7.7 | 28.8 | 0.5 | 25.4 |
| 125 s | 1 | 19.99 | 49.1 | 1.9 | 23.2 | 0.4 | 23.7 |

At 118 s the bots were exiting (docker CPU already down); by 125 s only the Unity client remained after the 5 s grace period.

### Unity client under load
- **Join:** `[net] joined playerId=22 name=EditorA pos=(-0.89,9.75)`, state `InWorld`.
- **Movement:** W held 4.7 s, then D 2.3 s, then release. Avatar went from (-0.89, 9.75) to (10.61, 33.26): +23.5 m z and +11.5 m x, consistent with 5 m/s. Back to `Idle` after release.
- **Crowd:** 150 avatars rendered with name labels; 149 bot avatars in the `Run` animation state at the same instant.
- **After the bots exited:** Unity client still `InWorld`, avatar count back to 1.

Screenshots:
- [T8.3-before.png](./screenshots/T8.3-before.png) — edit mode, empty zone
- [T8.3-after-crowd.png](./screenshots/T8.3-after-crowd.png) — Unity client's follow camera in the crowd (`EditorA` label in yellow)
- [T8.3-after-overview.png](./screenshots/T8.3-after-overview.png) — whole 100 × 100 m zone from above with all 150 labeled avatars

### Two-rendered-client check (T8.2)
A = Unity editor (`EditorA`, id 20), B = standalone Windows build (`Build\dumb-client.exe -name BuildB`, id 19), same server.

| Check | Result |
|---|---|
| B sees A move | A held W 3.01 s (z −5.00 → 9.75). In B's window A is below B before and above B after. |
| Latency during the check (`bot --clients 1 --duration 20`) | 1/1 connected, 0 drops, 20.0 Hz, avg 28.2 ms, **p95 51 ms** ✅ (386 samples, including A's and B's `t0` echoes) |
| Distinct ids | B = 19, A = 20 in A's snapshot registry |

Screenshots: [T8.2-B-before.png](./screenshots/T8.2-B-before.png), [T8.2-B-after.png](./screenshots/T8.2-B-after.png) (B's window, captured with `PrintWindow`, no input sent), [T8.2-A-before.png](./screenshots/T8.2-A-before.png), [T8.2-A-after.png](./screenshots/T8.2-A-after.png).

### Server test suite (after the run, with MySQL)
`cargo test -p server` with `TEST_DATABASE_URL` set: **19 passed, 0 failed, 0 skipped** — 11 unit, 1 `echo`, 1 `persistence`, 6 `zone_e2e`.

### Requirement acceptance criteria
| # | Acceptance criterion (REQUIREMENTS.md) | Verification | Result |
|---|---|---|---|
| 1 | One Unity client joins with a valid name → gets own player ID + initial position, enters world | Run 2: `[net] joined playerId=22 … pos=(-0.89,9.75)`, `NetworkClient` state `InWorld` | ✅ |
| 2 | Rejoining player gets last saved position | Live: `EditorA` ended T8.2 at (-0.89, 9.75) and rejoined in Run 2 at exactly (-0.89, 9.75). Test: `persistence::rejoin_restores_position_and_duplicate_names_share_a_profile` passed | ✅ |
| 3 | Two Unity clients: A moves → B sees it at 20 Hz, end-to-end latency < 100 ms | T8.2: B's screenshots show A's new position; bot cross-check 20.0 Hz, p95 51 ms | ✅ |
| 4a | 150 connections: server accepts all (no rejections or forced drops) | Run 1: 150/150 bots, 0 drops. Run 2: 149/149 bots + Unity client, 0 drops, `players` = 150 | ✅ |
| 4b | Every client receives snapshots with all 150 players at ~20 Hz | Recv rate 20.0 Hz per bot (both runs); Unity client registry held 150 entries; latency samples at 99.3 % / 99.8 % of the all-peers expectation (see caveats) | ✅ |
| 4c | p95 end-to-end latency < 100 ms | Run 1: 58 ms. Run 2: 58 ms | ✅ |
| 4d | Server CPU/RAM within the demo machine's budget, recorded here | ≈ 10 % of one core (≈ 0.3 % of 32 threads), < 36 MiB container memory, in both runs | ✅ |
| 5 | Disconnect + grace expiry → removed from snapshots, position persisted, others told they left | Live: after the bots exited, `players` 150 → 1 and the Unity client's avatars 150 → 1. Tests: `zone::close_suspends_for_grace_then_removes_with_player_left`, `zone_e2e::join_move_peer_and_disconnect`, `persistence::…` passed | ✅ |
| 6 | 1 human Unity client + 149 bots → multi-unit gameplay observed with 150-connection validation | Run 2: Unity client walked under WASD among 150 animated, labeled avatars (crowd + overview screenshots) while 149/149 bots held p95 58 ms | ✅ |
| 7 | Duplicate display name → both accepted, distinguishable by unique player ID | Tests: `zone_e2e::duplicate_names_are_both_accepted_with_distinct_ids`, `persistence::…` (two "Bob"s, distinct ids, shared profile) passed. Not exercised live in Run 2 | ✅ |

### Caveats
- **"Human" client is MCP-driven.** The Unity client stands in for the human: it joins through its real UI and moves through the real `MovementInput` path via synthetic Input System keyboard events, but no person was at the keyboard.
- **Frame time is the editor, not a player build.** Measured in the Unity editor Game view on an RTX 4070 Ti SUPER. The first two 5 s samples each contained one ~1 s frame caused by `execute_code` compiling the sampler; the reported 10 s sample skips its first 10 frames and has a 16.8 ms worst frame.
- **`cpuPercent` occasionally reads ~0.** At 43 s, 73 s and 103 s `/metrics` reported 0.0–0.4 % while `docker stats` showed 7–9 %. The server's `sysinfo` sampling glitches on some reads; `docker stats` is the reliable CPU source, and the peak numbers above agree between both.
- **Criterion 4b is indirect for bots.** Bots do not assert a player count per snapshot. The evidence is the 20 Hz receive rate plus latency sample counts matching "every peer's new `seq` seen by every bot" to within 1 %. The Unity client's registry held all 150 directly.
- **Walk durations drifted.** Planned 4 s W + 3 s D; actual 4.7 s + 2.3 s because key changes are applied from the editor update callback, which runs less often than frames under play mode.
- **Overview screenshot uses a temporary camera.** `T8.3-after-overview.png` is a positioned capture, so the overlay UI (status line) is absent; `T8.3-after-crowd.png` is the real client view with UI.
- **Latency caveats from Run 1 still apply:** one shared clock, includes 0–50 ms tick wait, and bots, server, and Unity all share this machine.
- **No deviation from the 100 ms target.** p95 was 58 ms.

---

## Run 3 — Network simulation: baseline vs LATENCY_MS=100 JITTER_MS=40

Evidence for [REQUIREMENTS-netsim.md](./REQUIREMENTS-netsim.md).

**Date:** 2026-09-15 13:05 -04:00

### Machine
Same machine as Run 2: Windows 11 Home 10.0.26200, Intel Core i9-14900K (24 cores / 32 logical), 63.8 GB RAM, Rust 1.90.0. Server and all 150 bots ran on this one machine; both runs back to back in the same session.

### Build under test
- `cargo build --release -p server -p bot` from the working tree with the netsim tasks T1.1–T3.3 applied (no git commits in this project)
- Native `target\release\server.exe`, **in-memory** (no `DATABASE_URL`, no MySQL, no Docker)

### Commands
From `dumb-server/` in PowerShell:
```
cargo build --release -p server -p bot

# Bad values (log check only)
$env:LATENCY_MS='abc'; $env:JITTER_MS='-5'; target\release\server.exe    # stopped after startup log

# Baseline
Remove-Item Env:LATENCY_MS, Env:JITTER_MS
target\release\server.exe
target\release\bot.exe --clients 150 --duration 60 --json ..\docs\netsim-baseline.json

# Delayed
$env:LATENCY_MS='100'; $env:JITTER_MS='40'
target\release\server.exe
target\release\bot.exe --clients 150 --duration 60 --json ..\docs\netsim-100-40.json
```

### Results
| Metric | Baseline (0/0) | Delayed (100/40) | Target |
|---|---|---|---|
| Connected | **150/150** | **150/150** | 150/150 both |
| Forced drops | **0** | **0** | 0 both |
| Snapshot recv rate (avg per client) | **20.0 Hz** | **20.0 Hz** | ≥ 19 Hz both |
| Latency samples | 13,363,680 | 13,329,258 | — |
| One-way latency avg | 30.8 ms | 168.9 ms | — |
| One-way latency p95 | 46 ms | 189 ms | — |
| **Latency avg delta** | — | **+138.1 ms** | +100 to +150 ms |
| Server `memBytes` at end of run | 14.1 MiB | 41.7 MiB | — |
| Server `snapshotsPerSec` at end of run | 19.97 | 20.14 | — |

Raw bot reports: [netsim-baseline.json](./netsim-baseline.json), [netsim-100-40.json](./netsim-100-40.json).

### Startup log lines
Bad values (`LATENCY_MS=abc JITTER_MS=-5`), no warning line followed:
```
INFO server::config: effective config bind=0.0.0.0:8080 tick_hz=20 grace_ms=5000 speed=5.0 world_half=50.0 database_url="(none)" latency_ms=0 jitter_ms=0
```
Delayed (`LATENCY_MS=100 JITTER_MS=40`):
```
INFO server::config: effective config bind=0.0.0.0:8080 tick_hz=20 grace_ms=5000 speed=5.0 world_half=50.0 database_url="(none)" latency_ms=100 jitter_ms=40
WARN server::config: network simulation active: all connections delayed latency_ms=100 jitter_ms=40
```

### Requirement acceptance criteria
| # | Acceptance criterion (REQUIREMENTS-netsim.md) | Verification | Result |
|---|---|---|---|
| 1 | 0/0 → no delay, every existing `cargo test` passes unchanged | Full `cargo test` green after every task; existing e2e tests use `NetSim::default()` (bypass). `netsim_e2e::no_delay_when_off` (join → joined < 100 ms). `netsim::zero_config_releases_immediately` | ✅ |
| 2 | 200/0 → `joined` ≥ 200 ms after `join` | `netsim_e2e::joined_reply_delayed_by_round_trip`. Teeth check: with `latency_ms: 0` it fails (312 µs) | ✅ |
| 3 | Single-frame delay within `[L/2, L/2+J/2]` | `netsim::delay_within_bounds` (100/40, 200 iterations in [50, 70] ms); `netsim::idle_gap_does_not_accumulate` | ✅ |
| 4 | 1000 frames under L=0 J=1000 released in push order | `netsim::preserves_order_under_jitter` | ✅ |
| 5 | `LATENCY_MS=abc` / `-5` → runs with 0, log shows 0 | Bad-values log line above | ✅ |
| 6 | Non-zero values → warning names them | Warning log line above | ✅ |
| 7 | Queued frames delivered before close; queued inbound frames processed | `netsim_e2e::reply_delivered_after_leave` (junk + `leave` both queued; `bad_message` arrives before close). `netsim::next_is_cancel_safe` | ✅ (amended — see caveats) |
| 8a | 150/150 connected both runs | Results table | ✅ |
| 8b | 0 forced drops both runs | Results table | ✅ |
| 8c | Recv rate ≥ 19 Hz both runs | 20.0 Hz both | ✅ |
| 8d | Latency avg +100 to +150 ms over baseline | +138.1 ms | ✅ |
| 9 | Docs: README table, compose, demo-run.md | README "Server configuration" rows `LATENCY_MS`/`JITTER_MS`; `docker/docker-compose.yml` `server.environment`; this section | ✅ |

### Caveats
- **Criterion 7 amended during implementation.** A client-sent WebSocket Close frame drops queued *outbound* frames: axum's tungstenite 0.29 marks the socket `ClosedByPeer` when it reads the Close and rejects later data frames with `SendAfterClosing`. Queued inbound frames are still handled. The same limit exists with simulation off. Verified via `leave` instead.
- **Delta above the +120 ms mean.** Expected mean added one-way latency is 100 + 40/2 = 120 ms; measured +138 ms. Still inside the accepted range. Not investigated further; the design names order clamping under jitter as the first suspect, and baseline/delayed runs also differ in tick alignment noise.
- **Memory rises with simulation on** (14 → 42 MiB), from frames held in the per-connection queues. Not a requirement; noted for the 150-player budget.
- **Baseline isn't comparable to Runs 1–2.** Native in-memory server here vs Docker + MySQL there, so 30.8 ms avg / 46 ms p95 is not a regression check against 34.9 / 58 ms.
- **Latency caveats from Run 1 still apply:** one shared clock, includes 0–50 ms tick wait, bots and server share this machine.
