# Requirements: Network Simulation — Latency & Jitter

## Goal
Everything in the demo runs on localhost, so client/server behavior under real network conditions can't be observed. Add a server-side simulator that gives every connection (Unity client and bots) configurable latency and jitter. This lets netcode be prototyped and judged under realistic round-trip times.

## Scope
**In scope:**
- Server-side delay of WebSocket **data frames** (text/binary) in both directions, per connection. Outbound goes through `writer.rs`, inbound through `ingest.rs`.
- Global configuration through the environment variables `LATENCY_MS` and `JITTER_MS`.
- Frame order is preserved.
- Unit and integration tests, a 150-bot baseline-vs-delayed run, and doc updates (README env table, `docker/docker-compose.yml`, `docs/demo-run.md`).

**Out of scope:**
- Per-connection or runtime-adjustable settings.
- Separate uplink and downlink settings.
- Packet loss, reordering, duplication, bandwidth limits. The transport is TCP, so reordering is not realistic anyway.
- Delaying WebSocket control frames (ping/pong/close handshake).
- Unity client changes, such as a latency display or client-side simulation.
- Changes to the bot's latency measurement.

## Inputs
- `LATENCY_MS`: integer ms, default `0`. Added round-trip time.
- `JITTER_MS`: integer ms, default `0`. Maximum extra random round-trip time.
- A value that is unparseable or negative falls back to `0`, which matches the existing `env_parse` behavior. There is no upper cap.
- The frames delayed are the text/binary frames each connection sends and receives.

## Outputs
- Each data frame, in each direction, is released after `LATENCY_MS/2 + uniform[0, JITTER_MS/2]` ms. The added round trip therefore falls in `[LATENCY_MS, LATENCY_MS + JITTER_MS]`.
- A frame never overtakes an earlier frame on the same connection in the same direction. Its release time is `max(previous release, now + delay)`.
- The startup "effective config" log includes both values. When either value is non-zero, a warning log says network simulation is active.

## Acceptance criteria
- **Given** `LATENCY_MS` and `JITTER_MS` unset or `0`, **when** the server runs, **then** frames are not delayed and every existing `cargo test` passes unchanged.
- **Given** `LATENCY_MS=200`, `JITTER_MS=0`, **when** a client sends `join`, **then** the `joined` reply arrives ≥ 200 ms after the send (100 ms uplink + 100 ms downlink).
- **Given** `LATENCY_MS=L`, `JITTER_MS=J`, **when** the delay queue schedules a frame with no earlier frame pending, **then** its delay is within `[L/2, L/2 + J/2]` ms.
- **Given** a large jitter (e.g. `L=0`, `J=1000`) and ≥1000 frames pushed in sequence, **when** they are released, **then** they come out in push order.
- **Given** `LATENCY_MS=abc` or `LATENCY_MS=-5`, **when** the server starts, **then** it runs with latency `0`, and the effective-config log shows `0`.
- **Given** non-zero `LATENCY_MS` or `JITTER_MS`, **when** the server starts, **then** a warning log names the active values.
- **Given** frames still queued when a connection ends (`leave`, socket close, or the ingest loop ending), **when** it closes, **then** the queued frames are still delivered at their scheduled times before the socket closes. Any queued inbound frames, including `leave`, are still processed by the Zone. The existing grace-period/`Closed` behavior is unchanged.
  - **Amended during implementation:** when the *client* sends a WebSocket Close frame, queued **outbound** frames are dropped. axum's tungstenite (0.29) marks the socket `ClosedByPeer` on reading the Close and rejects further data frames with `SendAfterClosing`. Queued inbound frames are still processed by the Zone. The same limitation exists with simulation off. Delivery-before-close is verified via `leave`.
- **Given** the server with `LATENCY_MS=100`, `JITTER_MS=40` and 150 bots for 60 s, **when** it is compared with a baseline run (`0`/`0`) in the same session on the same machine, **then** all of the following hold:
  - 150/150 bots connected in both runs.
  - 0 forced drops in both runs.
  - Receive rate ≥ 19 Hz, so the delay causes no extra broadcast-lag frame skips.
  - Bot `latency avg` is higher than baseline by **+100 to +150 ms**.
- **Given** the finished feature, **when** the docs are read, **then** the following cover it:
  - The README server-config table documents `LATENCY_MS` and `JITTER_MS`.
  - `docker/docker-compose.yml` exposes them.
  - `docs/demo-run.md` records both bot reports and the pass/fail check.

## Constraints & dependencies
- The Rust server uses axum 0.8 `ws` and tokio. No new crate dependencies unless the design justifies one.
- The 150-player-per-zone performance requirement still holds, and there is no measurable regression with the simulation off.
- The zone broadcast channel buffer is 4 frames (`zone.rs`). The writer must keep draining it while frames wait in the delay queue, or it will lag and drop snapshots.
- The bot latency metric is the one-way `t0` echo (sender uplink + tick wait + receiver downlink). It is the source of the measured effect.
- Workflow is set in `CLAUDE.md`: technical design, then taskmaster plan, then one task at a time. No git add/commit/push.

## Open questions
- Unity client and bot behavior under very large values (e.g. `LATENCY_MS` > 1000) is not investigated: connect/join timeouts may trip. It is not needed for acceptance. Revisit if large values get used.
