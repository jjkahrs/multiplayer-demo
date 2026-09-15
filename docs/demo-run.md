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

---

## Run 4 — Client prediction & snapshot interpolation (LATENCY_MS=200 JITTER_MS=80)

Evidence for [REQUIREMENTS-prediction.md](./REQUIREMENTS-prediction.md).

### Before (T1.1) — current client, no prediction

**Date:** 2026-09-15 14:38 -04:00

**Machine:** same as Run 3 (Windows 11 Home 10.0.26200, i9-14900K, 63.8 GB, RTX 4070 Ti SUPER). Server, 149 bots and the Unity editor on one machine.

**Build under test:** working tree before any prediction task (no source changes). Native `target\release\server.exe`, in-memory. Unity 6000.6.0f1 editor, `Assets/Scenes/Demo.unity`, driven through Unity MCP.

**Commands** (PowerShell, from `dumb-server/`):
```
cargo build --release -p server -p bot
$env:LATENCY_MS='200'; $env:JITTER_MS='80'; target\release\server.exe
target\release\bot.exe --clients 149 --duration 300    # frame-time sample
target\release\bot.exe --clients 149 --duration 600    # key→motion sample (first batch expired)
```
Unity: Play → `execute_code` sets `NameField` = `EditorA` and invokes `JoinButton` → `InWorld`, 150 avatars.

**Server startup log:** `WARN server::config: network simulation active: all connections delayed latency_ms=200 jitter_ms=80`.

**Bot report (first batch, 300 s):** 149/149 connected, 0 drops, 20.0 Hz, one-way latency avg 286.9 ms, p95 327 ms.

#### Results
| Metric | Before |
|---|---|
| Avatars rendered | 150 |
| Frame time (10 s, first 10 frames skipped, 1,534 frames) | **6.52 ms avg (153.4 fps)**, p95 7.20 ms, p99 7.53 ms, max 16.35 ms |
| Start latency: `wKey.isPressed` → avatar moved > 1 mm | **+45 frames, 292.6 ms** |
| Start: walk animation (`Animator` `Moving`) | +45 frames (same frame as first motion) |
| Stop latency: `wKey.isPressed` false → position stops changing (first frame of a ≥ 60-frame run with < 1 mm/frame) | **+73 frames, 1,401.8 ms** |
| Stop: idle animation | +34 frames |
| Walk | W held 5.01 s, (0.00, 0.00) → (0.00, 25.22) |

Screenshot mid-walk: [prediction-before.png](./screenshots/prediction-before.png).

#### Method
- **Frame time:** an `Application.onBeforeRender` hook created via `execute_code` records `Time.unscaledDeltaTime` for 10 s after skipping 10 frames. The hook lives in the one call, so no `execute_code` compile lands inside the sample.
- **Key→motion:** a second `onBeforeRender` hook waits for the local avatar to be still for 144 frames, then sets W with `InputState.Change(Keyboard.current, new KeyboardState(Key.W))`. Each frame it stamps `Time.frameCount` when `wKey.isPressed` first reads true, when the avatar first moves > 1 mm and when `Animator.GetBool("Moving")` goes true. It captures the screenshot 2 s into the walk, releases W after 5 s, and stamps the release, stop and idle frames the same way.

#### Caveats
- **Synthetic key path changed from Run 2.** `InputSystem.QueueStateEvent` did not reach play-mode code: with the editor unfocused, `editorInputBehaviorInPlayMode = PointersAndKeyboardsRespectGameViewFocus` sends keyboard events to editor updates only. `execute_code` saw `wKey.isPressed` true while `MovementInput` and the per-frame hook saw false for 1,000+ frames, and the avatar did not move. Temporarily switching to `AllDeviceInputAlwaysGoesToGameView` + `IgnoreFocus` did not change that. `InputState.Change` from inside the player loop did work, and `MovementInput` still reads the key through `Keyboard.current`. The settings were restored afterwards; they were in-memory defaults (no settings asset).
- **Stop frame time is uneven.** The stop took 73 frames over 1,402 ms (~19 ms/frame), vs 45 frames over 293 ms (~6.5 ms/frame) at the start. MCP status polls were running during the stop and likely caused the slow frames. Treat ms as the primary stop number.
- **Stop includes the smoothing tail.** The exponential smoothing never reaches its target exactly, so "stopped" means < 1 mm/frame held for 60 frames.
- **Frame time and key→motion came from separate bot batches.** Both had 149 bots and 150 avatars.
- **Bot duration** was 300 s / 600 s instead of the plan's 120 s, to leave time for the MCP-driven sampling.

### After (T5.1) — local prediction + remote interpolation

**Date:** 2026-09-15 15:42 -04:00

**Build under test:** working tree with tasks T2.1–T4.2 applied, including the user-approved "age per direction run" amendment (see [task-plan-prediction.md](./task-plan-prediction.md) T2.2/T3.1). Native `target\release\server.exe`, rebuilt after the amendment, in memory. Same editor and scene; `Demo.unity` has `ZoneView.movementInput` assigned.

**Commands** (PowerShell, from `dumb-server/`):
```
cargo build --release -p server -p bot
$env:LATENCY_MS='200'; $env:JITTER_MS='80'; target\release\server.exe
target\release\bot.exe --clients 149 --duration 1800    # stopped after sampling
cargo test
```
Unity: Play → join `EditorA` → one `execute_code` `onBeforeRender` hook ran every measurement in sequence and wrote the results to a file. The hook was not polled while it ran, because each `execute_code` call compiles on the main thread and stalls the editor (see caveats).

**Server during the run:** `players=150`, 19.8–20.2 snapshots/s, tick dt 46–62 ms, 31–40 MB.

#### Results: before vs after
| Metric | Before (T1.1) | After (T5.1) |
|---|---|---|
| Avatars rendered | 150 | 150 |
| Frame time (10 s, first 10 frames skipped) | 6.52 ms avg (**153.4 fps**), p95 7.20, p99 7.53, max 16.35 (1,534 frames) | 7.01 ms avg (**142.7 fps**), p95 7.73, p99 10.43, max 16.86 (1,428 frames) |
| Start: `wKey.isPressed` → avatar moved > 1 mm | +45 frames, 292.6 ms | **+0 frames, 0.0 ms** |
| Start: walk animation | +45 frames | **+0 frames** |
| Stop: `wKey.isPressed` false → first frame with < 1 mm movement | (not measured this way) | **+0 frames** |
| Stop: first frame of a ≥ 60-frame run with < 1 mm/frame | +73 frames, 1,401.8 ms | +60 frames (see caveats) |
| Stop: idle animation | +34 frames | **+0 frames** |
| Straight walk: W held 5.01 s, (0.00, 15.03) → (0.00, 40.03) | — | 100 reconciles, **0 snaps**, max `LastError` 0.0045 m |
| Stop settle | — | Stop seq 4326 acked 272 ms after release. \|display − snapshot\| per snapshot: +0 ms 0.072, +80 ms 0.015, +129 ms 0.008, +176 ms 0.004, +203 ms 0.003, +236 ms 0.002. **0.002 m at +250 ms** |
| Remotes: 10 bots × 1,429 frames | — | All frame pairs: 14,280, **0** jumps > 5 × dt × 2 (worst 0.87×). Filtered straight-walk pairs (headings within 1°, > 2 m from bounds): 11,460, **0 reversals, 0 jumps** |

Screenshots: [prediction-before.png](./screenshots/prediction-before.png) (old client, mid-walk) · [prediction-after.png](./screenshots/prediction-after.png) (new client, mid-walk in the 150-avatar crowd). Per-task pairs: [T4.1-before](./screenshots/T4.1-before.png) / [T4.1-after](./screenshots/T4.1-after.png), [T4.2-before](./screenshots/T4.2-before.png) / [T4.2-after](./screenshots/T4.2-after.png).

#### Tests
- `cargo test` (workspace, no `TEST_DATABASE_URL`): **55 passed, 0 failed**. That is bot 2, protocol 6 + serde 16, server 20 unit, echo 1, netsim_e2e 3, persistence 1, zone_e2e 6.
- Unity EditMode `Demo.Tests.EditMode`: **20 passed, 0 failed**. That is `LocalPredictorTests` 6, `SnapshotInterpolatorTests` 5, `ProtocolTests` 6, `JoinScreenArgsTests` 3.

#### Requirement acceptance criteria
| # | Criterion (REQUIREMENTS-prediction.md) | Evidence | Result |
|---|---|---|---|
| 1 | Key press → position and facing change within 1 frame, walk animation the same frame | T5.1: move and `Moving` both +0 frames from `wKey.isPressed`, under 200/80 with 150 avatars. EditMode `SetInputThenAdvance_MovesAndFaces` | ✅ |
| 2 | Input to zero → stops within 1 frame; settles ≤ 0.3 m within 250 ms of the stop ack | First still frame +0, idle animation +0. \|display − snapshot\| 0.002 m at +250 ms. EditMode `Reconcile_AfterStop_ConvergesWithinTolerance` | ✅ |
| 3 | Walking straight → no correction snap (> 1 m) | 0 snaps in 100 reconciles over a 5 s walk (max error 0.0045 m). EditMode `Reconcile_SteadyWalk_ErrorNearZero` | ✅ |
| 4 | Remote on a straight segment away from bounds → never moves backward, no snaps | 11,460 filtered pairs across 10 bots: 0 reversals, 0 jumps. 0 jumps in all 14,280 pairs. EditMode `Interpolator_JitteredArrivals_Monotonic` | ✅ |
| 5 | Snapshot gap > render delay → extrapolate ≤ 50 ms, then hold at ≤ v × 50 ms past last known | EditMode `Interpolator_Underrun_ExtrapolatesThenHolds` (Interpolating → Extrapolating at 25 ms → Holding at 80 ms, held at newest + v × 0.05). Observed live in T4.2 during an 883 ms editor stall: all remotes went to `Holding` | ✅ |
| 6 | World bound → prediction clamps to ±worldHalf like the server | EditMode `MovementRules_ClampsToWorldHalf` (same cases as `player.rs` `clamps_to_world_bounds`) | ✅ |
| 7 | `cargo test` and Unity EditMode pass, including updated serde/Protocol and new prediction/interpolation tests | 55/55 and 20/20 above | ✅ |
| 8 | 150 bots + editor → fps ≥ baseline − 10 % | 142.7 fps vs 153.4 baseline = 93.0 % (floor 138.1) | ✅ |
| 9 | Before/after screenshots in `docs/screenshots/`, run recorded here | Links above; this section | ✅ |

#### Deviations & caveats
- **`ageMs` semantics amended (user-approved during T3.1).**
  - The design reset the age on every accepted input. The server snaps each input to the previous tick boundary, so the 100 ms same-direction resends moved the anchor 0–50 ms each time. The EditMode steady-walk test measured 0.25 m of error.
  - The server now resets `input_age` only when the direction changes, and the client consumes the age from the start of the same-direction run.
  - The requirements, both design docs and the task plan are updated.
- **The stop "60-frame run" metric shows +60 frames, while the first still frame is +0.**
  - After the stop ack (272 ms later), the tick-quantization correction fades over 100 ms. The settle log shows it dropping from 0.072 m to 0.002 m. That fade moves the avatar ≥ 1 mm on some frames, which restarts the run counter.
  - The avatar stops on the frame W is released; this is the ≤ 0.25 m residual the design expects.
  - The first-still-frame metric is the right comparison. The baseline used the 60-frame run only because exponential smoothing never stops exactly.
- **Frame time dropped 7 % vs the T1.1 baseline** (153.4 → 142.7 fps). It is within the 10 % budget and matches Run 2 (142.5 fps without netsim).
  - Part of the cost is the sampler itself, which records 10 bots per frame through reflection in the phases after the frame-time sample. The frame-time phase ran before that recording.
  - Editor-to-editor variance between runs was not measured.
- **Synthetic keys** go through `InputState.Change` inside the player loop (same as T1.1). `InputSystem.QueueStateEvent` doesn't reach play-mode code while the editor is unfocused.
- **Remote analysis** uses the render tick read in `onBeforeRender`. `ZoneView` computes its own in `Update` earlier in the same frame, so segment classification at snapshot boundaries can be off by one frame.
- **MCP polling stalls the editor.** A T4.2 diagnostic that polled during sampling hit an 883 ms stall, after which all remotes held and then caught up in 0.5 m / 0.17 m steps. All T5.1 numbers come from an unpolled hook.
- **Open item from T4.2.** A 3 s single-bot sample right after joining showed 3 jumps up to 2.36× the limit. It was not reproduced in the clean T4.2 re-sample or in T5.1 (0 in 14,280 pairs), and the cause is not established.
- **Existing `NetworkClient` reconnect bug** (not part of this work, not fixed). If the connection drops without `Disconnect()`, the old `SendLoop` stays alive and can swallow the first message of the next connection. Here that was the join, which left the client in `Joining`.
- **No bot report.** The 1,800 s bot run was stopped after sampling, so the bot report (latency, recv rate) wasn't printed. Server `/metrics` log lines are the evidence for 150 players at 20 Hz.
- Latency caveats from Run 1 still apply: one shared clock, and bots, server and Unity all on one machine.

## Run 5 — Client HUD: FPS & ping (T3.1 of task-plan-hud)

**Date:** 2026-09-15 −04:00
**Requirements:** [REQUIREMENTS-hud.md](./REQUIREMENTS-hud.md) · **Design:** [TECHNICAL_DESIGN-hud.md](./TECHNICAL_DESIGN-hud.md) · **Plan:** [task-plan-hud.md](./task-plan-hud.md)

### Setup
- Server: `cargo run --release -p server`, in memory, no bots. First with no simulation, then restarted with `$env:LATENCY_MS='100'; $env:JITTER_MS='0'`. The startup log confirmed `latency_ms=100 jitter_ms=0` and the "network simulation active" warning.
- Unity: editor play mode, `Demo.unity`, a single client driven through MCP `execute_code`. No mouse or keyboard.

### Method
- **FPS:** a single `Application.onBeforeRender` hook created in one `execute_code` call records `Time.unscaledDeltaTime` each frame for 6 s after entering `InWorld`.
  - When the displayed FPS number changes (after the first 1 s), it computes `frames / Σdt` over the preceding ≥ 0.5 s of frames and compares.
  - Results go to `SessionState` and are read back after an unpolled wait.
- **Ping:** the same kind of hook parses `HudText` every 0.5 s from 1 s to 6 s after `InWorld`. The first frame in world is also recorded, for the rejoin check.

### Results
| Check | Measured |
|---|---|
| FPS vs frame times (netsim off) | 3,429 frames, 9 HUD refreshes compared. Shown/actual: 582/582.0, 569/569.2, 553/553.2, 559/559.1, 523/522.9, 600/600.3, 577/577.0, 578/577.6, 594/594.3. **Max deviation 0.06 %** |
| Ping, netsim off (11 samples) | 58, 48, 40, 39, 33, 27, 22, 33, 28, 23, 7 ms |
| Disconnected (server killed) | `Disconnected`, `HudText` inactive, `StatusText` active ("Disconnected: The remote party closed the WebSocket connection…") |
| First frame in world after rejoin | `FPS: 0 / Ping: -- ms`, HUD active, `StatusText` inactive |
| Ping, `LATENCY_MS=100` (11 samples) | 156, 149, 144, 140, 141, 138, 161, 156, 166, 162, 155 ms |
| `cargo test` (after the run) | 55 passed, 0 failed |
| Unity EditMode (T2.1) | 32/32 |

Screenshots: [hud-before.png](./screenshots/hud-before.png) (`InWorld` status text top-left, no HUD) · [hud-after.png](./screenshots/hud-after.png) (`FPS: 605 / Ping: 50 ms`) · [hud-after-latency100.png](./screenshots/hud-after-latency100.png) (`FPS: 576 / Ping: 148 ms`).

### Requirement acceptance criteria
| # | Criterion | Evidence | Result |
|---|---|---|---|
| 1 | HUD hidden when not `InWorld` | T2.1 before-join read and this run's disconnected read: `HudText` inactive | ✅ |
| 2 | HUD top-left, no overlap | `hud-after.png`; `StatusText` inactive in world | ✅ |
| 3 | FPS within ±10 % | Max 0.06 % over 9 comparisons | ✅ |
| 4 | Ping 0–60 ms with netsim off, updating | 7–58 ms, 10 distinct values | ✅ |
| 5 | Ping 100–175 ms at `LATENCY_MS=100` | 138–166 ms | ✅ |
| 6 | Samples only on a new `seq` | EditMode `PingSampler_RepeatedSeq_SamplesOnce` (teeth-checked in T1.2) | ✅ |
| 7 | No local entry / no sample yet keeps value | EditMode `NoLocalEntry_KeepsLast`, `BeforeJoin_Null`, `SeqZero_NoSample` | ✅ |
| 8 | Rejoin shows `--` until a new sample | First in-world read after rejoin `Ping: -- ms`; EditMode `SetLocalPlayer_ClearsPing` | ✅ |
| 9 | Screenshots saved; existing tests pass | Links above; EditMode 32/32, `cargo test` 55 passed | ✅ |

### Caveats
- **Existing `NetworkClient` reconnect bug hit again** (same as the Run 4 caveat; not part of this work, not fixed).
  - After killing the server and restarting it with latency, the first rejoin stayed in `Joining`. The socket was `Established`, the server logged `players=0`, and 1 bot joined the same server fine.
  - Reflection on the client showed `sendQueue.Count=0` and `sendSignal.CurrentCount=0`: the join was consumed by the previous connection's `SendLoop`.
  - Re-sending `client.Join("HudLat")` joined immediately. All rejoin and latency numbers above come from that session.
- **Screenshot missed overlay UI once.** A capture taken while stuck in `Joining` showed only the ground, with no join panel even though `JoinPanel` was active. It was discarded. The retake after joining included the overlay as usual.
- **"PlayerLoop internal function has been called recursively" ×5** in the Unity console during this play session. It appeared around the server kill and the MCP calls. The cause was not investigated; HUD behavior and the samples were unaffected.
- **Localhost ping sits near the 60 ms ceiling at times** (max 58 ms). That is the up-to-50 ms tick wait plus up to one frame. A slower frame or MCP stall could push a sample past 60 without anything being wrong.
- **Absolute FPS (~520–630) is an empty zone in the editor.** It is not comparable to the 150-avatar runs.
