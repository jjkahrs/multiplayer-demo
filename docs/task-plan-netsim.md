# Task Plan: Network Simulation — Latency & Jitter

Source design: [TECHNICAL_DESIGN-netsim.md](./TECHNICAL_DESIGN-netsim.md)
Source requirements: [REQUIREMENTS-netsim.md](./REQUIREMENTS-netsim.md)

Build/test commands (run from `dumb-server/` unless stated):
- `cargo build` / `cargo test`: whole workspace (DB-backed persistence tests skip unless `TEST_DATABASE_URL` is set)
- `cargo test -p server netsim`: netsim unit + e2e tests once they exist
- `cargo build --release -p server -p bot`: release binaries in `target\release\`
- `target\release\server.exe`: in-memory server on `0.0.0.0:8080`. Set env vars in PowerShell first: `$env:LATENCY_MS='100'; $env:JITTER_MS='40'`
- `target\release\bot.exe --clients 150 --duration 60 --json <path>`: bot loader with report

## How to use this plan
Work tasks in ID order. Each task is self-contained: read its block, do it, check its acceptance criteria, then change its checkbox to `[x]`. Don't start a task until its dependencies are checked off.

Notes that apply across the whole plan:
- **Never** `git add` / `git commit` / `git push` (CLAUDE.md).
- The server is a Rust 2024 crate at `dumb-server/crates/server`. Match the existing style: `//!` module docs, doc comments on pub items, short `tokio::select!` loops, and `tracing` for logs.
- **Evidence:** this feature has no UI. Proof is the test output plus the two bot reports recorded in `docs/demo-run.md`. The user agreed there are no screenshots for this feature.
- **Parallel-safe:** T3.1 and T3.2 don't depend on each other.

## Progress
- [x] Phase 1 — Delay queue core
  - [x] T1.1 — Add `netsim.rs`: `NetSim`, `DelayQueue<T>`, unit tests
- [x] Phase 2 — Config and wiring (no behavior change)
  - [x] T2.1 — Add `net_sim` to `Config`, log/warn, README + compose
  - [x] T2.2 — Thread `NetSim` through router → connect → writer/ingest
- [x] Phase 3 — Live simulation
  - [x] T3.1 — Writer downlink delay + flush on close
  - [x] T3.2 — Ingest uplink delay + drain on stream end
  - [x] T3.3 — End-to-end netsim tests
- [x] Phase 4 — Acceptance
  - [x] T4.1 — Startup-log checks + 150-bot baseline vs 100/40 run, record in demo-run.md

---

## Phase 1 — Delay queue core

**Goal:** Build the timing core first and prove its riskiest properties in isolation: bounds, order and cancel-safety.
**Exit condition:** `cargo test -p server netsim::` passes all six unit tests on paused tokio time.

### [ ] T1.1 — Add `netsim.rs`: `NetSim`, `DelayQueue<T>`, unit tests

**Depends on:** none
**Files:** `dumb-server/crates/server/src/netsim.rs` (new), `dumb-server/crates/server/src/lib.rs` (add `pub mod netsim;`)

**Context:** The server has no network-simulation code. This module gives the per-connection `writer.rs` and `ingest.rs` a queue that holds frames until a simulated-network release time. Nothing uses it yet. tokio's `time` feature is enabled, and `test-util` is in dev-deps, so `#[tokio::test(start_paused = true)]` is available. There is no RNG crate in `crates/server/Cargo.toml`, and you must not add one.

**Do:**
- `#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)] pub struct NetSim { pub latency_ms: u64, pub jitter_ms: u64 }`. The doc comments say these are round-trip figures and each direction gets half.
- `impl NetSim { pub fn is_off(self) -> bool }`: true when both are 0.
- `pub struct DelayQueue<T> { sim: NetSim, pending: VecDeque<(tokio::time::Instant, T)>, last_release: tokio::time::Instant, rng: u64 }`.
- `new(sim)`:
  - Seeds `rng` from `std::hash::RandomState::new().build_hasher().finish()`.
  - Sets `last_release` to `Instant::now()`.
- Private `one_way_delay(&mut self) -> Duration`:
  - Returns `Duration::from_micros(latency_ms * 500 + next_u64() % (jitter_ms * 500 + 1))`.
  - `next_u64` is SplitMix64: `state += 0x9E3779B97F4A7C15`, then the standard mix.
  - Add the comment `// ponytail: SplitMix64, statistical quality irrelevant for jitter; swap to rand if distributions matter`.
- `push(&mut self, item: T)`:
  - Computes `release = max(last_release, now + one_way_delay())`.
  - Stores it in `last_release` and pushes `(release, item)` onto the back of `pending`.
- `pub async fn next(&mut self) -> T`:
  - If `pending` is empty, `std::future::pending().await`.
  - Otherwise `sleep_until(front.0).await`, **then** `pop_front`.
  - Popping only after the sleep keeps it cancel-safe inside `select!`. Say so in the doc comment.
- `pub fn is_empty(&self) -> bool`.
- `#[cfg(test)] mod tests`: every test is `#[tokio::test(start_paused = true)]` and measures with `tokio::time::Instant`.
  - `zero_config_releases_immediately`: `NetSim::default()`, push, `next()` → elapsed == 0.
  - `delay_within_bounds`: `{100, 40}`. Loop 200 times: push, `next()`, and assert elapsed ∈ [50 ms, 70 ms].
  - `preserves_order_under_jitter`: `{0, 1000}`, push `0..1000` at the same instant, collect 1000 `next()` results → equals `0..1000`.
  - `next_is_cancel_safe`: `{100, 0}`, push `7`. `tokio::time::timeout(1 ms, q.next())` returns `Err`, and a later `q.next().await` returns `7`.
  - `idle_gap_does_not_accumulate`: `{100, 0}`, push, `next()`, `sleep(1 s)`, push → the second `next()` elapsed is exactly 50 ms.
  - `is_off_only_when_both_zero`: `{0,0}` → true. `{1,0}` and `{0,1}` → false.

**Acceptance:**
- [ ] `cargo build` succeeds with no new warnings apart from dead-code warnings on the not-yet-used `netsim` items (acceptable until T3.1).
- [ ] `cargo test -p server netsim::` passes all six tests.
- [ ] `crates/server/Cargo.toml` is unchanged (no new dependency).
- [ ] The full `cargo test` stays green.

---

## Phase 2 — Config and wiring (no behavior change)

**Goal:** Carry `LATENCY_MS`/`JITTER_MS` from the environment down to every connection task, while traffic stays undelayed.
**Exit condition:** The server starts with the new values in its effective-config log, and `writer::run`/`ingest::run` receive `NetSim`. The full `cargo test` stays green.

### [ ] T2.1 — Add `net_sim` to `Config`, log/warn, README + compose

**Depends on:** T1.1
**Files:** `dumb-server/crates/server/src/config.rs`, `dumb-server/crates/server/src/zone.rs` (test helper literal ~line 276), `dumb-server/crates/server/tests/echo.rs` (literal ~line 10), `dumb-server/crates/server/tests/common/mod.rs` (literal ~line 26), `README.md` ("Server configuration" table ~line 132), `dumb-server/docker/docker-compose.yml` (`server.environment` ~line 32)

**Context:** `crates/server/src/netsim.rs` exists and defines `NetSim { latency_ms: u64, jitter_ms: u64 }` (Copy, Default) with `is_off()`. `Config::from_env()` in `config.rs` reads env vars through the private `env_parse(key, default)`, which falls back to the default when a value is unparseable. `Config::log()` emits one `tracing::info!` "effective config" line. Three places build `Config { .. }` literals and will stop compiling once the field is added.

**Do:**
- Add the field `pub net_sim: NetSim` with the doc comment `Simulated latency/jitter, LATENCY_MS / JITTER_MS (default 0/0 = off).`
- In `from_env`: `net_sim: NetSim { latency_ms: env_parse("LATENCY_MS", 0), jitter_ms: env_parse("JITTER_MS", 0) }`. `-5` and `abc` fail `u64` parsing, so both become 0.
- In `log()`:
  - Add the fields `latency_ms = self.net_sim.latency_ms` and `jitter_ms = self.net_sim.jitter_ms` to the existing info line.
  - After it, add `if !self.net_sim.is_off() { tracing::warn!(latency_ms, jitter_ms, "network simulation active: all connections delayed") }`.
- Add `net_sim: NetSim::default()` to the three literals: `zone.rs` `test_config()`, `tests/echo.rs`, `tests/common/mod.rs`.
- README table: add two rows after `WORLD_HALF`.
  - `LATENCY_MS | 0 | Simulated added round-trip ms (half each direction)`
  - `JITTER_MS | 0 | Max extra random round-trip ms; frame order preserved`
- `docker-compose.yml` `server.environment`: add `LATENCY_MS: "0"` and `JITTER_MS: "0"`.

**Acceptance:**
- [ ] `cargo test` (whole workspace) passes.
- [ ] `target\debug\server.exe` started with no env vars logs `latency_ms=0 jitter_ms=0` and no "network simulation active" warning. Stop it with Ctrl-C.
- [ ] Started with `$env:LATENCY_MS='100'; $env:JITTER_MS='40'`, it logs `latency_ms=100 jitter_ms=40` plus the warning line.
- [ ] Started with `$env:LATENCY_MS='abc'; $env:JITTER_MS='-5'`, it logs `latency_ms=0 jitter_ms=0` and no warning.
- [ ] The README table and compose file contain both variables.

### [ ] T2.2 — Thread `NetSim` through router → connect → writer/ingest

**Depends on:** T2.1
**Files:** `dumb-server/crates/server/src/http.rs`, `dumb-server/crates/server/src/writer.rs` (signature only), `dumb-server/crates/server/src/ingest.rs` (signature only), `dumb-server/crates/server/src/main.rs` (~line 49), `dumb-server/crates/server/tests/echo.rs` (~line 20), `dumb-server/crates/server/tests/common/mod.rs`

**Context:** `Config` now has `net_sim: NetSim` (from `crate::netsim`). Here is how connections work:
- `http::router(zone: ZoneHandle)` builds the axum `Router` with state `ZoneHandle`.
- `ws_upgrade` calls `connect(socket, zone)`, which splits the socket and spawns `writer::run(sink, unicast_rx, zone.outbound, player_id_tx, zone.events.clone())` and `ingest::run(stream, zone.events, unicast_tx, player_id_rx)`.

The design keeps simulation settings out of `ZoneHandle` (loose coupling), so they travel as separate router state.

**Do:**
- `http.rs`: `pub fn router(zone: ZoneHandle, net_sim: NetSim) -> Router` with state `(ZoneHandle, NetSim)`.
  - Update `metrics` to extract `State((zone, _))`. `/health` needs no state.
  - `ws_upgrade` extracts both and calls `connect(socket, zone, net_sim)`.
  - `connect` passes `net_sim` as the new **last** argument to `writer::run` and `ingest::run`.
- `writer.rs` / `ingest.rs`: add a trailing parameter `net_sim: NetSim` and don't use it yet. Name it `_net_sim` to avoid an unused warning; T3.1/T3.2 rename it.
- `main.rs`: `router(zone, config.net_sim)`.
- `tests/echo.rs`: `server::http::router(server::zone::spawn(&config, None), config.net_sim)`.
- `tests/common/mod.rs`:
  - Add `pub async fn start_server_with(pool: Option<MySqlPool>, net_sim: NetSim) -> String`, containing today's `start_server` body with `net_sim` put into the `Config` literal and passed to `router`.
  - `start_server(pool)` becomes `start_server_with(pool, NetSim::default()).await`.

**Acceptance:**
- [ ] `cargo build` succeeds with no new warnings.
- [ ] `cargo test` (whole workspace) passes. No test behavior changes.
- [ ] `target\debug\server.exe` runs, and `curl.exe http://127.0.0.1:8080/health` and `/metrics` both return 200 JSON.

---

## Phase 3 — Live simulation

**Goal:** Frames are actually delayed in both directions when `NetSim` is non-zero. The zero path is unchanged.
**Exit condition:** `cargo test` passes, including `tests/netsim_e2e.rs`, which proves the round-trip delay and delivery-before-close over real sockets.

### [ ] T3.1 — Writer downlink delay + flush on close

**Depends on:** T1.1, T2.2
**Files:** `dumb-server/crates/server/src/writer.rs`

**Context:**
- `writer::run(sink, unicast_rx, outbound, player_id_tx, events, _net_sim: NetSim)` loops with `tokio::select!` over two branches: the unicast `mpsc::Receiver<ServerMsg>` and the zone `broadcast::Receiver<Outbound>`. It encodes each frame to `Option<Message>` and calls `sink.send(frame)`.
- It subscribes to broadcasts only when it pulls the `joined` reply, so `joined` is the first frame.
- On `unicast_rx.recv() == None` (ingest ended) or `RecvError::Closed` it breaks, sends `ClientEvent::Closed` if joined, and closes the sink.
- The broadcast buffer is 4. The writer must keep pulling while frames wait, or `Lagged` skips snapshots.
- `crate::netsim::DelayQueue<T>` offers `new(NetSim)`, `push(T)`, a cancel-safe `async next() -> T` (pending when empty) and `is_empty()`. `NetSim::is_off()` is true for 0/0.

**Do:**
- Rename the parameter to `net_sim`. Create `let mut downlink = DelayQueue::new(net_sim);`.
- Add a third `select!` branch: `frame = downlink.next() => { if sink.send(frame).await.is_err() { <dead socket: skip flush, go to Closed/close> } continue; }`.
- After a frame is encoded in the existing flow (where `sink.send(frame)` happens today):
  - if `net_sim.is_off()`, send immediately as today
  - otherwise `downlink.push(frame)`
- **Flush.** Leaving the loop through unicast `None` or broadcast `Closed` means `while !downlink.is_empty() { if sink.send(downlink.next().await).await.is_err() { break } }`. Leaving through a dead socket skips the flush. After that, report `Closed` and `sink.close()` as today.
- Update the `//!` module doc: mention the downlink delay and the flush.

**Acceptance:**
- [ ] `cargo build` succeeds with no warnings. The `netsim` dead-code warnings for `DelayQueue` are gone.
- [ ] `cargo test` (whole workspace) passes. Existing e2e tests run with `NetSim::default()` (bypass).
- [ ] Code review check: every path that sends a frame goes either through `downlink` or through the `is_off()` bypass, and no `.await` on `sink.send` blocks pulling from the broadcast while `net_sim` is on (enqueueing is synchronous).

### [ ] T3.2 — Ingest uplink delay + drain on stream end

**Depends on:** T1.1, T2.2
**Files:** `dumb-server/crates/server/src/ingest.rs`

**Context:** `ingest::run(stream, events, unicast_tx, player_id_rx, _net_sim: NetSim)` loops on `stream.next()`:
- `Message::Text` is parsed into `ClientMsg`. Bad JSON gets an `error` reply through `unicast_tx`.
- `Join`, the first valid one, becomes `ClientEvent::Join`. `Input` becomes `ClientEvent::Input` once `player_id_rx` holds an id. `Leave` breaks.
- `Message::Close` breaks, and other non-text frames are skipped.
- A failed `events.send` breaks, because the Zone stopped.
- When `run` returns, `unicast_tx` is dropped, which makes the writer flush and report `Closed`.

`crate::netsim::DelayQueue<T>` offers `new`, `push`, a cancel-safe `async next()` and `is_empty()`. `NetSim::is_off()` is true for 0/0. Requirement: delay only data (text) frames, never control frames. Queued frames are still processed after the socket ends.

**Do:**
- Move the per-text-frame body into `async fn handle(text: Utf8Bytes, state: &mut Ingest) -> Flow`.
  - `struct Ingest { events, unicast_tx, player_id_rx, join_sent: bool }`.
  - `enum Flow { Continue, Stop }`. `Stop` covers `Leave` and a failed `events.send`.
- Rename the parameter to `net_sim`, and create `let mut uplink = DelayQueue::<Utf8Bytes>::new(net_sim);`.
- Main loop is a `tokio::select!` with two branches.
  - **Branch `frame = stream.next()`:**
    - `Some(Ok(Message::Text(t)))`: if `net_sim.is_off()`, call `handle(t)` (return on `Stop`). Otherwise `uplink.push(t)`.
    - `Some(Ok(Message::Close(_)))`, `Some(Err(_))` and `None`: break out of the reading loop, without delay.
    - Any other message: continue.
  - **Branch `t = uplink.next()`:** `handle(t)`, and return on `Stop`.
- After the loop: `while !uplink.is_empty() { if let Flow::Stop = handle(uplink.next().await, &mut state).await { break } }`, then return.
- Update the `//!` module doc: mention the uplink delay and the drain.

**Acceptance:**
- [ ] `cargo build` succeeds with no warnings.
- [ ] `cargo test` (whole workspace) passes, including the existing bad-input/duplicate-name tests (bypass path).
- [ ] Code review check: `Close`, `Ping` and `Pong` never enter `uplink`, and `leave` handled from the queue stops processing.

### [ ] T3.3 — End-to-end netsim tests

**Depends on:** T3.1, T3.2
**Files:** `dumb-server/crates/server/tests/netsim_e2e.rs` (new)

**Context:** The server now delays frames when `NetSim` is non-zero:
- Each direction delays `latency_ms/2 + uniform[0, jitter_ms/2]`, and order is preserved.
- The writer flushes queued frames before closing.
- Ingest drains queued frames after the client's Close.

`tests/common/mod.rs` provides:
- `start_server_with(pool, NetSim) -> "host:port"`
- `connect(addr)`, `send(client, ClientMsg)`, `send_raw(client, &str)`
- `recv(client) -> Option<ServerMsg>` (None on close, 2 s timeout)
- `expect_error(client, code)`

Include it with `mod common;`, like the other test files do.

**Do:**
- `joined_reply_delayed_by_round_trip`:
  - `start_server_with(None, NetSim { latency_ms: 200, jitter_ms: 0 })`, connect.
  - `let t = std::time::Instant::now();`, send `ClientMsg::Join { name: "Lag" }`.
  - Receive `ServerMsg::Joined`, then assert `t.elapsed() >= 200 ms` and `< common::TIMEOUT`.
- `reply_delivered_after_leave` (was `reply_delivered_after_client_close`; amended — tungstenite 0.29 rejects data frames after reading a client Close with `SendAfterClosing`):
  - Same server config. Connect, `send_raw(client, "not json")`, then `send(ClientMsg::Leave)`.
  - Keep reading: the client must receive `ServerMsg::ErrorMsg { code: "bad_message" }` (use `recv` and match) **before** `recv` returns `None`.
- `no_delay_when_off`:
  - `start_server_with(None, NetSim::default())`.
  - Time join → `joined` and assert `< 100 ms`. This is a regression guard for the bypass, with a generous bound for CI noise.

**Acceptance:**
- [ ] `cargo test -p server --test netsim_e2e` passes all three tests.
- [ ] Temporarily setting `latency_ms: 0` in the first test makes it fail, which confirms the assertion has teeth. Revert afterwards.
- [ ] `cargo test` (whole workspace) passes.

---

## Phase 4 — Acceptance

**Goal:** Show the effect under the real 150-player load and record the evidence.
**Exit condition:** `docs/demo-run.md` has a "Run 3" section with both bot reports and every requirement acceptance criterion marked pass/fail.

### [ ] T4.1 — Startup-log checks + 150-bot baseline vs 100/40 run, record in demo-run.md

**Depends on:** T3.3
**Files:** `docs/demo-run.md` (append "Run 3"), `docs/netsim-baseline.json` (new, bot output), `docs/netsim-100-40.json` (new, bot output)

**Context:**
- The server reads `LATENCY_MS`/`JITTER_MS`, logs them at startup and warns when either is non-zero.
- The bot `target\release\bot.exe --clients N --duration S --json <path>` prints and saves:
  - connections
  - forced drops
  - recv rate (Hz)
  - latency samples
  - latency avg ms
  - latency p95 ms

  Latency is the one-way `t0` echo (sender uplink + tick wait + receiver downlink), so 100/40 should add +100..+140 ms to the average.
- `docs/demo-run.md` has Run 1 and Run 2 sections, each with Machine, Build under test, Commands and Results table.
- Port 8080 may already be used by the Docker stack: `docker compose -f dumb-server/docker/docker-compose.yml ps`. If it is, stop the stack, or run with `$env:BIND='127.0.0.1:8081'` and pass `--server ws://127.0.0.1:8081/ws`.

**Do:**
1. `cargo build --release -p server -p bot`.
2. **Bad values:** start `target\release\server.exe` with `$env:LATENCY_MS='abc'; $env:JITTER_MS='-5'`. Copy the effective-config log line (expect `latency_ms=0 jitter_ms=0`, no warning), then stop it.
3. **Baseline:** clear both env vars (`Remove-Item Env:LATENCY_MS, Env:JITTER_MS`) and start the server. Run `target\release\bot.exe --clients 150 --duration 60 --json ..\docs\netsim-baseline.json`, then stop the server.
4. **Delayed:** start the server with `$env:LATENCY_MS='100'; $env:JITTER_MS='40'` and copy the warning log line. Run `target\release\bot.exe --clients 150 --duration 60 --json ..\docs\netsim-100-40.json`, then stop the server.
5. Append "## Run 3 — Network simulation: baseline vs LATENCY_MS=100 JITTER_MS=40" to `docs/demo-run.md`:
   - date, and machine (same as Run 2 unless changed)
   - build under test (release, in-memory, no MySQL)
   - exact commands
   - a Results table with both runs side by side: connected, forced drops, recv rate, samples, latency avg, latency p95, and the **avg delta**
   - the two quoted log lines
   - a pass/fail row for each criterion in `REQUIREMENTS-netsim.md`, linking the test names from T1.1/T3.3 for the test-verified ones
6. Update this plan's Progress checkboxes.

**Acceptance:**
- [ ] Both runs: 150/150 connected, 0 forced drops, recv rate ≥ 19 Hz.
- [ ] Latency avg (delayed) − latency avg (baseline) is between +100 and +150 ms. If it's outside that range, record FAIL, don't tune numbers, and flag it to the user. The design names jitter clamping as the first suspect.
- [ ] Both JSON reports exist under `docs/`, and Run 3 in `demo-run.md` quotes the two log lines and the pass/fail table.
- [ ] `cargo test` (whole workspace) still passes at the end.

---

## Open questions
- **Unity client under simulation** isn't part of acceptance (see the design's risks). If you want it, add an optional manual task: connect the Unity client to a 100/40 server, confirm it joins and moves, and take before/after screenshots.
- **Values above 1000 ms:** Unity and bot connect/join timeouts aren't investigated. That doesn't block this plan.
