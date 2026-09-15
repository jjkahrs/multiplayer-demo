# Dumb Multiplayer Demo

Rapid prototype of client ↔ server MMO communication: a Rust WebSocket server (MySQL-backed) authoritative over one zone, a Unity client, and a Rust bot loader that proves **150 players in one zone at 20 Hz with p95 latency < 100 ms**.

[Watch example with 150 connected bots + forced 200ms latency + forced 50ms jitter ](https://youtu.be/KkDHSZBbMSI)


- `dumb-server/` — Rust workspace: `protocol`, `server`, `bot` crates + docker-compose stack
- `dumb-unity-client/` — Unity 6000.6.0f1 client
- `docs/` — requirements, technical design, task plan, [demo run results](docs/demo-run.md)

## Prerequisites
| Requirement | Needed for | Notes |
|---|---|---|
| **Windows 10/11** | everything | Tested on Windows 11. Commands below are PowerShell. The standalone client build targets Windows 64-bit. |
| **Docker Desktop** (with Compose) | server + MySQL | Tested with Docker 29.3.0. Docker must be running. |
| **Rust toolchain** via [rustup](https://rustup.rs) | bot loader, running the server without Docker, tests | Tested with 1.90.0. Needs ≥ 1.88 (`sysinfo 0.38`; crates use edition 2024). |
| **Unity Hub** + **Unity 6000.6.0f1** | Unity client | Install that exact editor version from Unity Hub. Windows build support (Mono) ships with the Windows editor; no extra module is needed. |
| **Git** on `PATH` | Unity client | Unity Package Manager fetches the `com.coplaydev.unity-mcp` package from a Git URL. |
| Free ports **8080** and **3306** | server, MySQL | |
| Hardware | 150-player run | Recorded results used an i9-14900K / 64 GB; server, bots and Unity all ran on one machine. |

## Install
1. **Get the code** into a local folder, e.g. `C:\dev\multiplayer-demo`. All commands below run from the repo root unless a step says otherwise.
2. **Build and start the server + MySQL** (first run downloads `mysql:8.4` and compiles the server image, which takes a few minutes):
   ```powershell
   docker compose -f dumb-server/docker/docker-compose.yml up --build -d
   ```
3. **Build the bot loader** (release build):
   ```powershell
   cargo build --release -p bot --manifest-path dumb-server/Cargo.toml
   ```
4. **Open the Unity project:** Unity Hub → **Add** → **Add project from disk** → select `dumb-unity-client`, then open it with Unity **6000.6.0f1**. The first open imports all packages (Input System, URP, uGUI/TextMeshPro, glTFast for the character model) and can take several minutes.
5. **(Optional) Build the standalone client:** in Unity, **File → Build Profiles → Windows** → **Build**, output to `dumb-unity-client/Build/dumb-client.exe`. Only needed to run a second rendered client outside the editor.

## Run the demo
1. **Check the server is up:**
   ```powershell
   curl.exe http://127.0.0.1:8080/health     # {"status":"ok"}
   ```
   Use `curl.exe`, not `curl` — PowerShell aliases `curl` to `Invoke-WebRequest`. On first start the server waits for MySQL to report healthy (~30 s).
2. **Join from Unity:** open `Assets/Scenes/Demo.unity`, press **Play**, type a display name (1–16 characters), click **Join**. The host field defaults to `ws://127.0.0.1:8080/ws`. Click the Game view, then move with **WASD**. Your name label is bold yellow.
3. **(Optional) Second rendered client:** run the standalone build, which joins automatically from its launch arguments:
   ```powershell
   dumb-unity-client\Build\dumb-client.exe -screen-fullscreen 0 -screen-width 1280 -screen-height 720 -name PlayerB
   ```
   Add `-host ws://<server-ip>:8080/ws` to point it at another server.
4. **Fill the zone with 149 bots** (1 Unity client + 149 bots = 150 players):
   ```powershell
   dumb-server\target\release\bot.exe --clients 149 --duration 120
   ```
   Bots appear as `Bot-<i>` avatars walking around. When the run ends they disconnect and disappear after the 5 s grace period.
5. **Stop:** press **Play** again in Unity to exit play mode, then:
   ```powershell
   docker compose -f dumb-server/docker/docker-compose.yml down
   ```
   > **Warning:** adding `-v` (`down -v`) also deletes the MySQL volume — every saved player profile and last position is permanently lost.

Expected on the recorded machine: 149/149 bots connected, 0 drops, ≈ 20 Hz, p95 ≈ 58 ms, server ≈ 10 % of one core; Unity editor ≈ 140 fps with 150 avatars. See [docs/demo-run.md](docs/demo-run.md) for full results and screenshots.

Returning players (same display name) spawn at their last saved position.

## Running the 150-bot load test

Uses the prerequisites above (Docker, Rust). Commands below are PowerShell, run from `dumb-server/`.

### 1. Start MySQL + server
```powershell
cd dumb-server
docker compose -f docker/docker-compose.yml up --build -d
```
The server waits for MySQL to report healthy (first start can take ~30 s while `init.sql` creates the schema). Check it is up:
```powershell
curl.exe http://127.0.0.1:8080/health     # {"status":"ok"}
```
Use `curl.exe`, not `curl` — PowerShell aliases `curl` to `Invoke-WebRequest`.

### 2. Build the bot loader (release)
```powershell
cargo build --release -p bot
```
Use a release build for 150 bots. Each bot parses a 150-player snapshot 20×/s; a debug build can load the bot process enough to inflate measured latency.

### 3. Run 150 bots
```powershell
target\release\bot.exe --clients 150 --duration 60 --json ..\docs\demo-run-150.json
```

| Flag | Default | Meaning |
|---|---|---|
| `--server` | `ws://127.0.0.1:8080/ws` | Server WebSocket endpoint |
| `--clients` | `1` | Number of concurrent bots |
| `--duration` | `30` | Run length in seconds |
| `--move` | `random` | Movement pattern (only `random`) |
| `--json` | — | Also write the report as JSON to this path |

Each bot joins as `Bot-<i>`, random-walks at 10 Hz (new direction every 1–3 s), and measures one-way latency from other players' echoed `t0`. When the run ends, bots send `leave` and disconnect.

### 4. Read the report
```
connections            150/150 connected
forced drops           0
recv rate (avg)        20.0 Hz
latency samples        13313074
latency avg            34.9 ms
latency p95            58 ms
server /metrics        {"cpuPercent":9.663182,"lastTickDtMs":49.195417,"memBytes":11943936,"players":150,"snapshotsPerSec":20.010820536749495,"tickCount":1928}
```

Pass criteria (from the requirements):
- `connections` = **150/150** and `forced drops` = **0**
- `recv rate` ≈ **19–20 Hz**
- `latency p95` **< 100 ms**

Example above is from the recorded run in [docs/demo-run.md](docs/demo-run.md) (i9-14900K, 64 GB). Bot and server must run on the **same machine**: latency uses a shared wall clock (`t0` echo), so cross-machine numbers are only relative.

### 5. Watch the server while it runs (optional)
In a second terminal:
```powershell
curl.exe http://127.0.0.1:8080/metrics
docker stats --no-stream multiplayer-demo-server-1
docker compose -f docker/docker-compose.yml logs -f server   # [metrics] line every 5 s
```
`/metrics` fields: `players`, `tickCount`, `lastTickDtMs`, `snapshotsPerSec`, `cpuPercent`, `memBytes`. CPU % uses 100 % = one full core.

### 6. Stop the stack
```powershell
docker compose -f docker/docker-compose.yml down
```
> **Warning:** `down -v` also deletes the MySQL volume — every saved player profile, including the `Bot-*` rows, is permanently lost. Use it only when you want a clean database.

## Server configuration
Environment variables read by the server (set them under `server.environment` in `docker/docker-compose.yml`, or in the shell for `cargo run -p server`):

| Variable | Default | Meaning |
|---|---|---|
| `BIND` | `0.0.0.0:8080` | Listen address |
| `DATABASE_URL` | unset | MySQL URL; unset = no persistence |
| `TICK_HZ` | `20` | Simulation/snapshot rate |
| `GRACE_MS` | `5000` | Disconnect grace before a player is removed |
| `SPEED` | `5.0` | Walk speed, m/s |
| `WORLD_HALF` | `50` | World half-size, m (world is ±50 m) |
| `LATENCY_MS` | `0` | Simulated added round-trip ms (half each direction) |
| `JITTER_MS` | `0` | Max extra random round-trip ms; frame order preserved |

### Running without Docker
```powershell
cargo run --release -p server                          # in-memory, no MySQL
target\release\bot.exe --clients 150 --duration 60     # second terminal
```

## Simulating network latency and jitter
Everything runs on localhost, so by default there is almost no network delay. Set `LATENCY_MS` / `JITTER_MS` on the server to delay every connection (Unity clients and bots) as if it were on a real network.

How it works:
- The server delays each WebSocket data frame, in both directions. Each direction gets `LATENCY_MS/2 + random[0, JITTER_MS/2]` ms, so the added **round trip** is between `LATENCY_MS` and `LATENCY_MS + JITTER_MS`.
- Frames never overtake each other. Ping/pong/close control frames are not delayed.
- Settings are global and read once at startup. To change them, restart the server.
- `0`/`0` (the default) turns simulation off completely. Bad values (`abc`, `-5`) also fall back to `0`.

### Local server
```powershell
$env:LATENCY_MS='100'; $env:JITTER_MS='40'
cargo run --release -p server
```
Turn it off again with `Remove-Item Env:LATENCY_MS, Env:JITTER_MS` and restart the server.

### Docker stack
`docker/docker-compose.yml` reads both variables from your shell (default `0`):
```powershell
$env:LATENCY_MS='100'; $env:JITTER_MS='40'
docker compose -f docker/docker-compose.yml up -d
```
Compose only picks up the new values when it recreates the container, so run `up -d` again after changing them.

### Check it is on
The startup log shows the effective values, plus a warning whenever simulation is active:
```
INFO server::config: effective config ... latency_ms=100 jitter_ms=40
WARN server::config: network simulation active: all connections delayed latency_ms=100 jitter_ms=40
```
(`docker compose -f docker/docker-compose.yml logs server` for the Docker stack.)

### Measure the effect
Run the bot loader against the delayed server and compare with a `0`/`0` run. The bot's `latency avg` is a one-way `t0` echo (sender uplink + tick wait + receiver downlink), so `100`/`40` should add roughly **+100 to +150 ms**. Recorded run: 30.8 ms → 168.9 ms (+138 ms), 150/150 connected, 20.0 Hz. See Run 3 in [docs/demo-run.md](docs/demo-run.md).

Caveats:
- If the client closes the socket with a WebSocket Close frame, any server replies still waiting in the delay queue are dropped. Messages the client already sent are still processed. Sending `leave` first avoids this.
- Values above ~1000 ms are untested. Client and bot connect/join timeouts may trip.
- Server memory grows while simulation is on, because frames wait in per-connection queues (≈ 14 → 42 MiB with 150 bots at 100/40).

## Troubleshooting
- **`0/150 connected`** — server not reachable. Check `curl.exe http://127.0.0.1:8080/health` and `docker compose -f docker/docker-compose.yml ps`.
- **Port 8080 already in use** — stop the other process, or run a local server on another port (`$env:BIND='127.0.0.1:8081'; cargo run -p server`) and pass `--server ws://127.0.0.1:8081/ws`.
- **Bots report `forced drops`** — the server closed connections mid-run; check `docker compose -f docker/docker-compose.yml logs server`.
- **High p95** — confirm the bot is a release build and the machine isn't busy with other load; bots and server share the CPU.

## Tests
```powershell
cargo test    # whole workspace; DB-backed persistence tests skip unless TEST_DATABASE_URL is set
```
