# Requirements: Client Prediction & Snapshot Interpolation

## Goal
The Unity client doesn't handle latency or jitter. Local input waits for the server tick, then the snapshot round trip, then exponential smoothing (`ZoneView.cs`). As a result, starting, stopping and turning show up 150–350 ms late under `LATENCY_MS=100`. Remote avatars have no buffer, so snapshots that jitter bunches together show up as uneven motion. This work removes the perceived input delay for the local player and the jitter-induced unevenness for remote players. Netcode can then be judged under the netsim's simulated conditions.

## Scope
**In scope:**
- **Protocol:**
  - `joined` gains `speed`, `worldHalf` and `tickHz`.
  - `snapshot` gains `tick`, a u64 that is monotonic per zone.
  - Each snapshot player entry gains `ageMs`: how long the server has simulated the current direction, through the acked `seq`. Added during design, because plain seq replay drifts when resends bump `seq`. Amended during T3.1: it resets only when the direction changes, because resetting on every resend re-quantized it to a tick boundary each time.
  - Files touched: Rust `protocol` crate, server `zone.rs`, serde tests, Unity `Protocol.cs` and `ProtocolTests`.
- **Local player prediction:**
  - Uses the server's movement rule: normalized dir × speed × dt, clamped to ±worldHalf.
  - Keeps a buffer of unacked inputs keyed by `seq`.
  - Reconciles on each snapshot: server position plus a replay of inputs with `seq` > the echoed `seq`.
- **Local correction:** residual error is blended out over ~100 ms. When the error is > 1 m, the avatar snaps instead. Both values are serialized and tunable.
- **Local visuals:** facing and walk/idle animation follow the predicted input, not the snapshot.
- **Remote players:**
  - Interpolation buffer keyed by `tick`.
  - Render delay is fixed and serialized, default 100 ms.
  - On underrun, extrapolate at last velocity for ≤ 50 ms, then hold.
- **Verification support:** key-to-motion latency is measured by an MCP `execute_code` sampler that sends synthetic Input System key events through the real `MovementInput` path, the same method as Run 2. Amended during design: no debug hook in production code.
- **Tests:** EditMode tests for the prediction, reconciliation and interpolation logic.

**Out of scope:**
- Adaptive interpolation delay.
- Lag compensation, server-side rewind, hit detection.
- Packet loss handling. The transport is TCP.
- Changes to the server tick rate, the input rate cap, or bot behavior, apart from compiling with the new fields.
- A latency/jitter HUD.
- Using the `playerJoined`/`playerLeft` events.

## Inputs
- **Local:** WASD direction (real or synthetic Input System key events), read every frame. It is sent as today (`MovementInput.cs`: on change plus every 100 ms) with an incrementing `seq`.
- **Server `joined`:** `playerId, name, x, z, yaw, speed, worldHalf, tickHz`.
- **Server `snapshot`:** `tick` plus players `{id, name, x, z, yaw, state, seq, t0, ageMs}`. Sent at `tickHz`. Arrives with latency and jitter but is never reordered.

## Outputs
- The local avatar's transform and animation update every frame from prediction and are corrected toward the reconciled server state.
- Remote avatar transforms are interpolated at `(latest tick time − render delay)`.
- During the acceptance run, the MCP sampler logs the measured key-to-motion latency in frames and ms.

## Acceptance criteria
Test conditions unless stated otherwise: server `LATENCY_MS=200`, `JITTER_MS=80`.
- **Given** a local player at rest, **when** a synthetic direction key press is processed, **then** the avatar's position and facing change within 1 rendered frame, and the walk animation starts in the same frame.
- **Given** a moving local player, **when** input changes to zero, **then**:
  - the avatar stops within 1 rendered frame;
  - the reconciled position settles within 0.3 m of the server's authoritative position, within 250 ms of the server acking the stop.
- **Given** a local player walking straight, **when** snapshots arrive, **then** no correction snap (> 1 m) occurs.
- **Given** a remote bot inside a straight-walk segment, away from the world bounds, **when** it is rendered, **then** its position along the path never moves backward from one frame to the next, and no snaps occur.
- **Given** a snapshot gap longer than the render delay, **when** the buffer underruns, **then**:
  - the remote avatar extrapolates for ≤ 50 ms, then holds;
  - it never moves further than last velocity × 50 ms past its last known position.
- **Given** a player reaching the world bound, **when** its movement is predicted, **then** the position is clamped to ±worldHalf, matching the server.
- **Given** the new protocol fields, **when** `cargo test` and the Unity EditMode tests run, **then** all pass. This includes the updated serde/Protocol tests and the new prediction/interpolation tests.
- **Given** 150 bots plus the editor client, **when** compared with a baseline measured before the change on the same machine, **then** the editor frame rate is ≥ baseline − 10%.
- **Given** the finished feature, **then** before/after screenshots are saved under `docs/screenshots/`, and the verification run is recorded in `docs/demo-run.md`.

## Constraints & dependencies
- **Client:** Unity 6000.6.0f1 with `JsonUtility`, whose fields map 1:1 to camelCase wire names.
- **Server:** Rust with axum/tokio. No new crates.
- **Expected divergence:** the server integrates once per 50 ms tick, while the client predicts every frame. Some divergence is expected, up to about speed × one tick ≈ 0.25 m. Correction blending must absorb it.
- **Rate cap:** the server drops inputs sent < 10 ms apart (`zone.rs`). The client sends at most on change plus 10 Hz, so there is no conflict. Replay must only drop buffered inputs that the server has acked by `seq`.
- **Coding standards:** loose coupling and event-driven design, so prediction and interpolation subscribe to `NetworkClient` events. Use an FSM where entity states apply.
- **Performance:** 150 players per zone must still hold.
- **Verification method:** never use the mouse or keyboard. Use input injected through MCP.
- **Workflow:** technical design, then the taskmaster plan, then one task at a time. No git add/commit/push.

## Open questions
- Baseline editor FPS under `LATENCY_MS=200 JITTER_MS=80` with 150 bots has not been measured yet. Run 2 measured 142.5 fps without netsim. Capture the baseline before wiring in the changes (design step 6).
