# Task Plan: Client Prediction & Snapshot Interpolation

Source design: [TECHNICAL_DESIGN-prediction.md](./TECHNICAL_DESIGN-prediction.md)
Source requirements: [REQUIREMENTS-prediction.md](./REQUIREMENTS-prediction.md)

Build/test commands:
- **Workspace build and tests.** From `dumb-server/`: `cargo build` / `cargo test`. DB-backed tests skip unless `TEST_DATABASE_URL` is set.
- **Release binaries.** From `dumb-server/`: `cargo build --release -p server -p bot`.
- **Server under simulation.** In PowerShell, from `dumb-server/`: `$env:LATENCY_MS='200'; $env:JITTER_MS='80'; target\release\server.exe`. This runs in memory on `0.0.0.0:8080`.
- **Bots.** From `dumb-server/`: `target\release\bot.exe --clients 149 --duration 120`.
- **Unity tests.** MCP `run_tests`, mode EditMode, assembly `Demo.Tests.EditMode`.

## How to use this plan
Work tasks in ID order. Each task is self-contained: read its block, do it, check its acceptance criteria, then change its checkbox to `[x]`. Don't start a task until its dependencies are checked off.

Notes that apply across the whole plan:
- **Never** `git add` / `git commit` / `git push` (CLAUDE.md).
- **Unity is agent-driven through Unity MCP.** Never use the OS mouse or keyboard. Drive input in-engine with `execute_code` and `InputSystem.QueueStateEvent(Keyboard.current, new KeyboardState(Key.W))`; release with `new KeyboardState()`. Scene: `Assets/Scenes/Demo.unity`. Join by setting `NameField` text and invoking `JoinButton.onClick` via `execute_code`.
- **Screenshots.** MCP writes only inside the Unity project. Capture to `dumb-unity-client/Captures/<task>-<before|after>.png`, then copy to `docs/screenshots/`. Every Unity task has a before/after pair.
- **Style.**
  - Rust: `//!` module docs, doc comments on pub items.
  - C#: namespace `Demo`, `[SerializeField] private` fields, event-driven wiring.
  - Mark deliberate shortcuts with `// ponytail:` comments.
- **Coordinates.** Logic classes use `Vector2(x, z)`. Yaw is in radians in the server convention `atan2(dir_z, dir_x)`. It converts to a rotation with `Quaternion.LookRotation(new Vector3(cos(yaw), 0, sin(yaw)))`.
- **Parallel-safe:** T3.1, T3.2 and T4.1 have no dependency on each other.

## Progress
- [x] Phase 1 — Baseline
  - [x] T1.1 — Measure the current client under 200/80 (frame time, key→motion latency, screenshot)
- [x] Phase 2 — Protocol
  - [x] T2.1 — Add Rust protocol fields with placeholder values
  - [x] T2.2 — Server semantics: tick counter, input age, movement rules in `joined`
  - [x] T2.3 — Unity `Protocol.cs` fields and tests
- [x] Phase 3 — Client logic (EditMode-tested, not wired)
  - [x] T3.1 — `MovementRules` + `LocalPredictor`
  - [x] T3.2 — `TickClock` + `SnapshotInterpolator`
- [x] Phase 4 — Wire-in
  - [x] T4.1 — `NetworkClient.SetInput` returns seq; `MovementInput.OnInputSent`
  - [x] T4.2 — Rewrite `ZoneView` to use the predictor and interpolators; scene reference
- [x] Phase 5 — Acceptance
  - [x] T5.1 — Acceptance run under 200/80, Run 4 in `demo-run.md`, wire-example doc update

---

## Phase 1 — Baseline

**Goal:** Record today's behavior under simulated latency before any change, so the before/after comparison is real.
**Exit condition:** `docs/demo-run.md` has a Run 4 "Before" section with frame time and key→motion numbers under 200/80, and `docs/screenshots/prediction-before.png` exists.

### [ ] T1.1 — Measure the current client under 200/80

**Depends on:** none
**Files:** `docs/demo-run.md` (append a Run 4 "Before" section), `docs/screenshots/prediction-before.png` (new)

**Context:**
- **The client today.** It has no prediction. `ZoneView.cs` exponentially smooths each avatar toward its newest snapshot. `MovementInput.cs` sends WASD to the server.
- **The simulator.** The server's `LATENCY_MS`/`JITTER_MS` add round-trip delay.
- **Run 2 of `docs/demo-run.md`** measured 142.5 fps with 150 avatars and no netsim, using an in-engine 10 s frame-time sample. Use the same method here.
- **Why now.** Code changes come later, so this number is the baseline for the "FPS ≥ baseline − 10 %" criterion.

**Do:**
- `cargo build --release -p server -p bot`. Start the server with `LATENCY_MS=200 JITTER_MS=80`, then run 149 bots for 120 s.
- Unity: play `Demo.unity`, and join as `EditorA` via `execute_code`.
- **Frame time:** an `execute_code` sampler over 10 s. Skip the first 10 frames. Report avg, p95, p99 and max ms.
- **Key→motion latency:** `execute_code` records the local avatar's position and frame index, queues W, then polls each frame (up to 1 s, via `EditorApplication.update` or a temporary coroutine host). Record:
  - the frame where `Keyboard.current.wKey.isPressed` becomes true;
  - the frame where the avatar position first changes by more than 1 mm;
  - the ms between them.

  Then release W and measure stop latency the same way: the frame where the position stops changing.
- **Screenshot** mid-walk: `Captures/prediction-before.png`, copied to `docs/screenshots/`.
- Write Run 4 "Before" in `demo-run.md`: date, commands, the numbers above, caveats.

**Acceptance:**
- [ ] `docs/demo-run.md` Run 4 "Before" lists frame time (avg/p95/p99/max), start latency (frames + ms) and stop latency (frames + ms) under 200/80 with 150 avatars.
- [ ] `docs/screenshots/prediction-before.png` exists and shows the local avatar among bots.
- [ ] No source files changed.

---

## Phase 2 — Protocol

**Goal:** The wire carries `tick`, `ageMs` and the movement rules end to end, and both sides' tests assert them.
**Exit condition:** `cargo test` is green; the Unity EditMode `ProtocolTests` are green with the new fields; the current client still runs against the new server.

### [ ] T2.1 — Add Rust protocol fields with placeholder values

**Depends on:** none
**Files:**
- `dumb-server/crates/protocol/src/messages.rs`
- `dumb-server/crates/protocol/tests/serde.rs`
- `dumb-server/crates/server/src/player.rs`
- `dumb-server/crates/server/src/zone.rs`
- `dumb-server/crates/bot/src/bot.rs`
- `dumb-server/crates/server/tests/common/mod.rs`
- `dumb-server/crates/server/tests/zone_e2e.rs`

**Context:**
- `messages.rs` defines `SnapshotPlayer {id, name, x, z, yaw, state, seq, t0}`, `ServerMsg::Joined {player_id, name, x, z, yaw}` and `ServerMsg::Snapshot {players}`. Serde renames to camelCase.
- `serde.rs` asserts exact JSON strings and round-trips.
- Several places match `ServerMsg::Snapshot { players }` with no `..`, so adding a field breaks them.

**Do:**
- **`SnapshotPlayer`:** add `pub age_ms: u64`, documented as "Milliseconds the server has integrated input `seq` (sum of tick dt since it was accepted)."
- **`Joined`:** add `speed: f64`, `world_half: f64`, `tick_hz: u64`, documented as movement rules for client prediction.
- **`Snapshot`:** add `tick: u64` before `players`, documented as a zone tick counter that is monotonic and starts at 0.
- **Server placeholders:**
  - `Player::snapshot()` emits `age_ms: 0`.
  - `zone.rs` `tick()` emits `tick: 0`.
  - `Zone::admit` fills `Joined` with `self.speed`, `self.world_half` and `tick_hz: 0`.
- **Patterns:** add `..` where they match `Snapshot { players }`: `bot.rs:95`, `tests/common/mod.rs:112`, `zone_e2e.rs:56`, `zone.rs:299`.
- **`serde.rs`:** update every literal and exact string. The new joined string is `{"type":"joined","playerId":7,"name":"Bob","x":0.0,"z":0.0,"yaw":1.5708,"speed":5.0,"worldHalf":50.0,"tickHz":20}`. Snapshot strings gain `"tick":…` before `"players"` and `"ageMs":…` after `"t0"`.

**Acceptance:**
- [ ] `cargo build` succeeds for the whole workspace, including `bot`.
- [ ] `cargo test -p protocol` passes, and the exact-string tests include `worldHalf`, `tickHz`, `tick` and `ageMs`.
- [ ] `cargo test` passes for the whole workspace.

### [ ] T2.2 — Server semantics: tick counter, input age, movement rules in `joined`

**Depends on:** T2.1
**Files:** `dumb-server/crates/server/src/player.rs`, `dumb-server/crates/server/src/zone.rs`

**Context:**
- T2.1 added `Snapshot.tick`, `SnapshotPlayer.age_ms` and `Joined.speed/world_half/tick_hz`, but the server emits `0` placeholders for `tick`, `age_ms` and `tick_hz`.
- `Zone::tick(dt)` integrates Active players once per tick with the real elapsed `dt`.
- `Player::apply_input` accepts or rejects input. It rejects magnitude > 1 or non-finite values, leaving `seq` unchanged.
- `spawn` passes `config.tick_hz.max(1)` only to `run`.
- **Purpose:** the client uses `age_ms` to know how much of its acked input the server has already simulated.

**Do:**
- **`Player`:** add `pub input_age: f64` (seconds), initialized to `0.0`. In `apply_input`, set `input_age = 0.0` only on the accepted path, next to `self.seq = seq`. `snapshot()` emits `age_ms: (self.input_age * 1000.0).round() as u64`.
- **`Zone`:** add `tick_hz: u64` (set in `spawn` from `config.tick_hz.max(1)`) and `tick: u64` (starts at 0).
- **`Zone::tick(dt)`:**
  - For every `Active` player, add `player.input_age += dt` **before** `integrate`, whether the player is moving or not.
  - Increment `self.tick` before building the snapshot, and emit it.
- **`Zone::admit`:** `Joined` gets `tick_hz: self.tick_hz`.
- **Unit tests:**
  - `player.rs` `input_age_resets_on_accepted_input_only`: set `input_age = 1.0`. A rejected input (`1.5, 0`) keeps `1.0`; an accepted one resets it to `0.0`.
  - `zone.rs` `snapshot_tick_increments_and_age_accumulates` (paused time, `test_config()`): join, send one input, collect consecutive snapshots. `tick` values increase by 1 each. After 3+ snapshots with no new input, the player's `age_ms` is within ±5 of the ticks elapsed since the input was accepted × 50.
  - `zone.rs` `joined_carries_movement_rules`: the `joined` reply has `speed == 5.0`, `world_half == 50.0`, `tick_hz == 20`.

**Acceptance:**
- [x] `cargo test -p server` passes, including the three new tests.
- [x] `cargo test` passes for the whole workspace.
- [x] Teeth check: temporarily removing the `input_age = 0.0` reset makes `input_age_resets_on_accepted_input_only` fail. Restore it afterwards.

**Amendment (during T3.1, user-approved):**
- **Change.** `input_age` now resets only when an accepted input changes the direction. Same-direction resends still update `seq`/`t0`. The test is renamed `input_age_resets_on_direction_change_only` and covers: a rejected input keeps the age, same direction keeps it, a new direction resets it, and a resend keeps it.
- **Why.** Resetting on every accepted input re-anchored the age to the previous tick boundary on each 100 ms resend. The T3.1 steady-walk test measured 0.25 m of error.
- **Verification.** Re-ran the teeth check on the new test: with the reset removed it fails (`new direction resets the age`, left 1.0, right 0.0), and with it restored `cargo test` is green.

### [ ] T2.3 — Unity `Protocol.cs` fields and tests

**Depends on:** T2.1, T2.2 (the play-mode check runs against the T2.2 server)
**Files:** `dumb-unity-client/Assets/Scripts/Protocol/Protocol.cs`, `dumb-unity-client/Assets/Tests/EditMode/ProtocolTests.cs`

**Context:**
- `Protocol.cs` mirrors `dumb-server/crates/protocol/src/messages.rs` with `[Serializable]` classes parsed by `JsonUtility`. Field names must equal the camelCase wire names.
- T2.1 added:
  - `joined`: `speed`, `worldHalf`, `tickHz`
  - `snapshot`: `tick`
  - snapshot player: `ageMs`
- `ProtocolTests` parses JSON literals copied from the server's exact-string tests.

**Do:**
- `ServerJoined`: add `public double speed; public double worldHalf; public long tickHz;`.
- `ServerSnapshot`: add `public long tick;`.
- `ServerPlayer`: add `public long ageMs;`.
- Update the `ParsesJoined` literal to `{"type":"joined","playerId":7,"name":"Bob","x":0.0,"z":0.0,"yaw":1.5708,"speed":5.0,"worldHalf":50.0,"tickHz":20}` and assert the three new fields.
- Update the `ParsesSnapshot` literal to include `"tick":1234` and `"ageMs":150`, and assert both.
- **Before/after screenshots:** capture the Test Runner or the console via MCP screenshot of the editor if available. Otherwise record the `run_tests` output summary in the task notes. This task has no visual change.

**Acceptance:**
- [x] MCP `read_console` shows no compile errors.
- [x] `run_tests` EditMode `Demo.Tests.EditMode`: all pass, including the updated `ParsesJoined` and `ParsesSnapshot`.
- [x] Play mode against the T2.2 server (no netsim): join succeeds (`[net] joined` in console) and WASD still moves the avatar. This proves backward-compatible behavior before the wire-in.

**Notes (2026-09-15):**
- **Before:** EditMode 9/9 passed on the old `Protocol.cs`.
- **After:** after the forced refresh the console had no errors or warnings, and EditMode passed 9/9, including the updated `ParsesJoined` and `ParsesSnapshot`.
- **Play mode:** against the T2.2 release server with no netsim, the console logged `[net] joined playerId=1 name=EditorA pos=(0.00,0.00)`. W held for 1 s moved the avatar from (0.00, 0.00) to (0.00, 5.04).
- **Key injection:** W was set through `InputState.Change` inside an `onBeforeRender` hook; see the Run 4 caveat for why `QueueStateEvent` doesn't work while the editor is unfocused.
- **Screenshots:** none, since this task has no visual change.

---

## Phase 3 — Client logic

**Goal:** Prediction, reconciliation and interpolation exist as plain C# classes, proven by EditMode tests, with no scene changes.
**Exit condition:** `run_tests` EditMode is green, including the new predictor/clock/interpolator tests; the game still behaves as before, because nothing is wired in yet.

### [ ] T3.1 — `MovementRules` + `LocalPredictor`

**Depends on:** T2.3
**Files:**
- `dumb-unity-client/Assets/Scripts/Game/Prediction/MovementRules.cs` (new)
- `dumb-unity-client/Assets/Scripts/Game/Prediction/LocalPredictor.cs` (new)
- `dumb-unity-client/Assets/Tests/EditMode/LocalPredictorTests.cs` (new)

**Context:**
- **Server rule** (`dumb-server/crates/server/src/player.rs` `integrate`): `x = clamp(x + dir_x·speed·dt, ±world_half)`, the same for z, and `yaw = atan2(dir_z, dir_x)` when moving. It runs once per 50 ms tick.
- **Server fields:** `ServerJoined` now carries `speed`, `worldHalf` and `tickHz` (T2.3). Each `ServerPlayer` carries `seq`, the newest input the server accepted, and `ageMs`, how long the server has integrated that input (T2.2).
- **Client input:** sends a normalized direction with an incrementing `seq` on change, plus every 100 ms, so consecutive seqs often share a direction.
- **Scripts assembly** is `Demo.asmdef`, and tests reference it.

**Do:**
- **`MovementRules` (readonly struct):**
  - Fields `Speed`, `WorldHalf` (float) and `TickHz` (int).
  - Constructors from `ServerJoined` and from raw values.
  - `Vector2 Step(Vector2 pos, Vector2 dir, float dt)` mirrors `integrate`, including the per-axis clamp.
- **`LocalPredictor`:**
  - Constructor `(MovementRules rules, Vector2 spawn, float yaw, float correctionTime, float snapDistance)`.
  - Properties: `DisplayPosition`, `PredictedPosition`, `Yaw`, `IsMoving`, `LastError`, `LastCorrectionSnapped`.
- **`SetInput(long seq, Vector2 dir)`:** stores the current seq and dir.
- **`Advance(float dt)`:**
  - Append a `Frame(seq, dir, dt)`, step `PredictedPosition`, and set `Yaw = atan2(dir.y, dir.x)` when `dir ≠ 0`.
  - `IsMoving = dir ≠ 0`.
  - Linearly decay the visual offset toward zero at `|offsetAtCorrection| / correctionTime` per second.
  - Cap the history at 1024 frames by dropping the oldest, with a `// ponytail:` comment that names the ceiling.
- **`Reconcile(long ackSeq, long ageMs, Vector2 serverPos)`:**
  1. Ignore an ack whose `ackSeq` is lower than the last reconciled ack.
  2. Drop frames with `seq < ackSeq`.
  3. Consume `ageMs/1000` seconds from the front frames with `seq == ackSeq`: remove the fully consumed ones and shorten the partly consumed one.
  4. Replay: `p = serverPos`, then `Step` through every remaining frame.
  5. Correct:
     - `error = p − PredictedPosition`, `LastError = |error|`.
     - If `LastError > snapDistance`: `offset = 0`, `LastCorrectionSnapped = true`.
     - Otherwise `offset -= error`, `LastCorrectionSnapped = false`. Record `offsetAtCorrection = offset`.
     - `PredictedPosition = p`.
- **`DisplayPosition`** = `PredictedPosition + offset`.
- **EditMode tests** (`LocalPredictorTests`). For the tests that need one, write a tiny in-test server model: it applies inputs at modeled arrival times and integrates at 20 Hz tick boundaries, crediting each tick's full dt to the current input and tracking age.
  - `MovementRules_ClampsToWorldHalf`: the same cases as `player.rs` `clamps_to_world_bounds`.
  - `SetInputThenAdvance_MovesAndFaces`: after one `Advance(1/144)`, the position changes and `Yaw ≈ π/2` for dir (0, 1).
  - `Reconcile_SteadyWalk_ErrorNearZero`: walk straight 3 s at 144 Hz with a seq resend every 100 ms. Deliver server snapshots at 20 Hz with a 100 ms one-way delay. Every `LastError` < 0.01 m, and snaps never happen.
  - `Reconcile_AfterStop_ConvergesWithinTolerance`: walk, then send dir 0. Within one `Advance` after `SetInput`, `PredictedPosition` stops changing. After the first snapshot acking the stop seq, `|DisplayPosition − server position|` ≤ 0.3 m within 250 ms.
  - `Reconcile_LargeError_Snaps`: a server position 5 m away sets `LastCorrectionSnapped`, and the display equals the predicted position.
  - `Reconcile_StaleAck_Ignored`: an older `ackSeq` after a newer one leaves the state unchanged.

**Acceptance:**
- [x] `read_console`: no compile errors.
- [x] `run_tests` EditMode: all `LocalPredictorTests` pass, and the existing tests still pass.
- [x] Teeth check: temporarily skipping the `ageMs` consumption (step 3) makes `Reconcile_SteadyWalk_ErrorNearZero` fail. Restore it afterwards.

**Notes (2026-09-15):**
- **First run, spec as written: `Reconcile_SteadyWalk_ErrorNearZero` failed at 0.25 m.** The server snapped every accepted input, including the 100 ms same-direction resends, to the previous tick boundary. That shifted the age anchor 0–50 ms on each resend, for an error of speed × (q₀ − q_s).
- **Fix (user-approved):** "age per direction run".
  - Server: `input_age` resets only when the direction changes (see the T2.2 amendment).
  - `Reconcile`: find ackSeq's same-direction run, drop the frames before it, and consume `ageMs − trimmed` from the run's front.
    - `trimmed` tracks how much of that run earlier reconciles or the history cap already removed.
    - If ackSeq's frames are gone, `trimmedSeq ≥ ackSeq` identifies its run as the trimmed one.
    - A seq with no frames at all (seq 0) skips nothing.
  - The in-test server model follows the same reset rule. The design doc's Reconcile algorithm and the requirements' `ageMs` wording are updated to match.
- **Result:** EditMode 15/15 passed (6 new `LocalPredictorTests`, 9 existing).
- **Teeth check:** with `skip = 0` the test failed (`Expected: less than 0.01, But was: 0.250000954`). After restoring, 15/15 passed again.
- **Compile check:** `Demo.dll` timestamps were checked after every recompile, since Unity doesn't always rebuild an assembly whose source didn't change.

### [ ] T3.2 — `TickClock` + `SnapshotInterpolator`

**Depends on:** none (pure logic, no protocol types)
**Files:**
- `dumb-unity-client/Assets/Scripts/Game/Prediction/TickClock.cs` (new)
- `dumb-unity-client/Assets/Scripts/Game/Prediction/SnapshotInterpolator.cs` (new)
- `dumb-unity-client/Assets/Tests/EditMode/SnapshotInterpolatorTests.cs` (new)

**Context:**
- **Snapshots:** the server broadcasts one every 50 ms (20 Hz) with a monotonic `tick`.
- **Delay:** the simulator adds a per-frame delay of latency/2 + uniform[0, jitter/2] each way and never reorders. A late frame holds back the ones behind it, so snapshots arrive bunched.
- **Rendering goal:** remote players render a fixed `renderDelay` (default 0.1 s) behind the server's timeline, interpolated between buffered samples. On underrun they extrapolate ≤ 50 ms, then hold.

**Do:**
- **`TickClock(int tickHz, double windowSeconds = 2.0)`:**
  - `OnSnapshot(long tick, double arrivalTime)` stores `(arrivalTime, offset = arrivalTime − tick/tickHz)` in a monotonic deque that keeps the minimum offset, and evicts samples older than `windowSeconds`.
  - `bool HasSync`.
  - `double RenderTick(double now, double renderDelaySeconds) = (now − minOffset − renderDelaySeconds) × tickHz`.
- **`SnapshotInterpolator(float maxExtrapolationSeconds = 0.05f, int tickHz = 20)`:**
  - Has `enum Mode { Interpolating, Extrapolating, Holding }` and a `CurrentMode` property.
  - `Add(long tick, Vector2 pos, float yaw, bool walking)` ignores a tick ≤ the newest one and keeps ≤ 32 samples.
  - `Sample(double renderTick, out Vector2 pos, out float yaw, out bool walking)`:
    - **Bracketed** (`a.tick ≤ renderTick ≤ b.tick`): `Interpolating`. Lerp position, shortest-angle yaw lerp, `walking = a.walking`. Trim samples older than `a`.
    - **Past newest, within `maxExtrapolation`:** `Extrapolating`. `v = (newest − prev) × tickHz / (newest.tick − prev.tick)`, `pos = newest + v × overshoot`.
    - **Beyond that:** `Holding`. `pos = newest + v × maxExtrapolation`, `walking = false`.
    - **One sample, or `renderTick` before the oldest:** hold that sample.
  - The mode is computed only inside `Sample`.
- **EditMode tests** (`SnapshotInterpolatorTests`):
  - `TickClock_MinOffsetIgnoresJitter`: arrivals with uniform 0–40 ms added delay (seeded `System.Random`). The derived offset equals the zero-delay offset ±1 ms.
  - `TickClock_WindowEviction`: after the lowest sample ages past the window, the offset rises.
  - `Interpolator_JitteredArrivals_Monotonic`:
    - A bot walks +x at 5 m/s. Snapshots arrive every 50 ms with 0–40 ms jitter plus the no-overtake clamp.
    - Feed them through `TickClock` + interpolator, and sample at 144 Hz over 5 s with `renderDelay` 0.1.
    - x never decreases, and no step exceeds `5 × dt × 2`.
  - `Interpolator_Underrun_ExtrapolatesThenHolds`: the modes go Interpolating → Extrapolating (overshoot 25 ms) → Holding (overshoot 80 ms), and the held position equals `newest + v × 0.05`.
  - `Interpolator_YawShortestPath`: from π−0.1 to −π+0.1, the midpoint is ≈ ±π, not 0.

**Acceptance:**
- [x] `read_console`: no compile errors.
- [x] `run_tests` EditMode: all `SnapshotInterpolatorTests` pass, and the existing tests still pass.
- [x] Teeth check: temporarily using the latest offset instead of the minimum makes `Interpolator_JitteredArrivals_Monotonic` or `TickClock_MinOffsetIgnoresJitter` fail. Restore it afterwards.

**Notes (2026-09-15):**
- **Result.** EditMode 20/20 passed: 5 new `SnapshotInterpolatorTests`, 15 existing.
- **Teeth check.** With the deque keeping only the latest sample, 3 tests failed:
  - `TickClock_MinOffsetIgnoresJitter` (tick 2: expected 10.00443, was 10.01868);
  - `TickClock_WindowEviction`;
  - `Interpolator_JitteredArrivals_Monotonic` ("moved backward at t=1.035").

  After restoring, 20/20 passed.
- **Deviation: `TickClock_MinOffsetIgnoresJitter`.** At every arrival it asserts the offset equals a brute-force minimum over the same 2 s window (±1e-9), and that it stays within the 40 ms jitter bound. The plan's "zero-delay offset ±1 ms" would depend on the seed: the 2 s window holds ~40 samples of U[0, 40 ms], so their minimum exceeds 1 ms about 36 % of the time.
- **Deviation: `Interpolator_JitteredArrivals_Monotonic`.** It skips the first second as warm-up. Until the window holds a low-delay sample, a new minimum legitimately pulls render time forward by more than one frame. It checks 500+ frames after that.
- **Extra assertion.** `TickClock` requires `HasSync` before `RenderTick`, and `SnapshotInterpolator.Sample` requires one `Add`. `ZoneView` guarantees both.

---

## Phase 4 — Wire-in

**Goal:** The running client predicts the local player and interpolates remote players.
**Exit condition:** In MCP play mode against a 200/80 server with bots, the local avatar moves the same frame W is processed, and the remote bots move smoothly. The console is clean.

### [ ] T4.1 — `NetworkClient.SetInput` returns seq; `MovementInput.OnInputSent`

**Depends on:** none (independent of T3.x; ordered here because T4.2 consumes it)
**Files:** `dumb-unity-client/Assets/Scripts/Networking/NetworkClient.cs`, `dumb-unity-client/Assets/Scripts/Input/MovementInput.cs`

**Context:**
- `NetworkClient.SetInput(float vx, float vz)` returns `void`, assigns `seq = ++seq` and sends when `InWorld`.
- `MovementInput.Update` polls WASD and calls `SetInput` on a direction change or every `resendInterval` (0.1 s).
- The upcoming `LocalPredictor` needs each sent `(seq, direction)` in the same frame, before `ZoneView.Update` runs.

**Do:**
- `public long SetInput(float vx, float vz)` returns the new `seq`, or `-1` when not `InWorld`, without incrementing.
- `MovementInput`:
  - add `[DefaultExecutionOrder(-10)]`;
  - add `public event Action<long, Vector2> OnInputSent;`;
  - after `SetInput` returns `seq >= 0`, invoke `OnInputSent?.Invoke(seq, direction)`.
- No other behavior change.

**Acceptance:**
- [x] `read_console`: no compile errors. `run_tests` EditMode green.
- [x] Play mode (server up, no netsim): join, then an `execute_code` subscribe to `OnInputSent` logs a line per send. Queue W for ~0.5 s. The console shows ≥ 5 lines with increasing seq and direction (0, 1), and the avatar moves as before.
- [x] Before/after screenshots `Captures/T4.1-before.png` / `T4.1-after.png` (mid-walk), copied to `docs/screenshots/`.

**Notes (2026-09-15):**
- **Server.** Release server rebuilt with the direction-run age change, running with no netsim.
- **Before (old code).** W held 1.0 s, (0.00, 0.00) → (0.00, 5.00), [T4.1-before.png](./screenshots/T4.1-before.png).
- **After: compile and tests.** No compile errors. Reflection confirms `MovementInput.OnInputSent` exists and `NetworkClient.SetInput` returns `Int64`. EditMode 20/20.
- **After: play mode.**
  - An `onBeforeRender` hook subscribed a logger to `OnInputSent` at press time and held W via `InputState.Change` for 0.6 s.
  - The console logged `[T4.1] OnInputSent seq=4..9 dir=(0.00,1.00)`: 6 lines, seq strictly increasing, ~48 frames apart (the 100 ms resend).
  - Then seq=10..19 `dir=(0.00,0.00)` for the release and the idle resends.
  - Avatar moved (0.00, 0.00) → (0.00, 2.96), as before at 5 m/s.
  - [T4.1-after.png](./screenshots/T4.1-after.png).
- **Screenshots.** Taken with no bots, since this task needs only the local avatar.

### [ ] T4.2 — Rewrite `ZoneView` to use the predictor and interpolators; scene reference

**Depends on:** T2.2, T3.1, T3.2, T4.1
**Files:**
- `dumb-unity-client/Assets/Scripts/Game/ZoneView.cs`
- `dumb-unity-client/Assets/Scenes/Demo.unity` (set the `movementInput` reference via MCP)

**Context:**
- **`ZoneView` today:**
  - subscribes to `NetworkClient` `OnJoined` / `OnSnapshot` / `OnDisconnected`;
  - keeps an `Entry {Avatar, TargetPosition, TargetRotation, State}` per player id;
  - spawns unknown ids, removes ids missing from a snapshot;
  - exponentially smooths every avatar in `Update` (`smoothing` field);
  - sets `cameraFollow.target` to the local avatar;
  - converts yaw with `LookRotation(cos(yaw), 0, sin(yaw))`.
- **Available now:**
  - `ServerJoined.speed/worldHalf/tickHz`; `ServerSnapshot.tick`; `ServerPlayer.seq/ageMs` (T2.3).
  - The server fills them with real values (T2.2).
  - `MovementRules` and `LocalPredictor(rules, spawn, yaw, correctionTime, snapDistance)` with `SetInput(seq, dir)`, `Advance(dt)`, `Reconcile(ackSeq, ageMs, serverPos)`, `DisplayPosition`, `Yaw`, `IsMoving` (T3.1).
  - `TickClock(tickHz)` with `OnSnapshot(tick, arrival)` and `RenderTick(now, delay)`, plus `SnapshotInterpolator(maxExtrapolation, tickHz)` with `Add` / `Sample` (T3.2).
  - `MovementInput.OnInputSent(long seq, Vector2 dir)` (T4.1).

**Do:**
- **Serialized fields:** add `MovementInput movementInput`, `float renderDelay = 0.1f`, `float maxExtrapolation = 0.05f`, `float correctionTime = 0.1f`, `float snapDistance = 1f`. Remove `smoothing`.
- **`HandleJoined`:**
  - store the local id;
  - build `MovementRules` from `joined`;
  - create `TickClock(rules.TickHz)` and `LocalPredictor` at `(joined.x, joined.z)` with `joined.yaw`;
  - subscribe `movementInput.OnInputSent += OnInputSent`, which forwards to the predictor.
- **`HandleSnapshot`:**
  - `clock.OnSnapshot(snapshot.tick, Time.realtimeSinceStartupAsDouble)`;
  - local id → `predictor.Reconcile(p.seq, p.ageMs, pos)`;
  - remote ids → the entry's `SnapshotInterpolator.Add(tick, pos, yaw, state == "walk")`, created at spawn;
  - spawn/remove unchanged.
- **`Update`:**
  - if a predictor exists, `Advance(Time.deltaTime)`, then set the local avatar's position, rotation and `SetState(IsMoving ? "walk" : "idle")`;
  - for each remote, `Sample(clock.RenderTick(now, renderDelay))`, then apply position, rotation and state.
- **`OnDisable` / `HandleDisconnected`:** unsubscribe `OnInputSent`, then null out the predictor and clock.
- **Yaw → rotation:** keep it in one private static helper.
- **Scene (MCP):** `manage_components set_property` on the ZoneView GameObject `movementInput` → the `MovementInput` component. Save the scene.
- **Before/after screenshots** mid-walk: `Captures/T4.2-before.png` / `T4.2-after.png`.

**Acceptance:**
- [x] `read_console`: no compile errors. `run_tests` EditMode green.
- [x] Server at `LATENCY_MS=200 JITTER_MS=80` with 5 bots. Play and join, then `execute_code` queues W and polls frames. The local avatar position changes within ≤ 1 frame of `wKey.isPressed` becoming true, and `Animator.GetBool("Moving")` is true the same frame.
- [x] Release W: the position stops changing within ≤ 1 frame. 1 s later the local avatar is within 0.3 m of its snapshot position (log both via `execute_code`).
- [x] One bot avatar sampled for 3 s: `CurrentMode` is mostly `Interpolating`, and no frame-to-frame jump exceeds `5 × dt × 2` m.
- [x] `Demo.unity` saved with `movementInput` assigned (`manage_components get` shows it). Screenshots are in `docs/screenshots/`.

**Notes (2026-09-15):**
- **Setup.** Release server rebuilt with the direction-run age, running at `LATENCY_MS=200 JITTER_MS=80`, with 5 bots.
- **Before (old ZoneView).** W held 3 s, (0.00, 0.00) → (0.00, 15.04), [T4.2-before.png](./screenshots/T4.2-before.png).
- **After: compile, tests, scene.**
  - No compile errors. Reflection confirms `ZoneView.movementInput` exists and `smoothing` is gone. EditMode 20/20.
  - The `Demo.unity` diff sets `movementInput: {fileID: 1850309260}` (DemoNet's `MovementInput`) and swaps `smoothing` for `renderDelay 0.1`, `maxExtrapolation 0.05`, `correctionTime 0.1` and `snapDistance 1`.
  - Resaving also serialized the `CameraFollow` zoom fields from the previous commit (`minZoom`/`maxZoom`/`zoomStep`), which were never saved before.
- **After: play mode.** One `onBeforeRender` hook injected W with `InputState.Change` and stamped frames.
  - **Start.** `wKey.isPressed` frame 35393, first motion frame 35393 (+0), `Moving` true frame 35393 (+0).
  - **Stop.** Release frame 36895; the position was still (< 1 mm) at frame 36895 (+0) and stayed still for ≥ 60 frames. The largest per-frame delta in the next 250 ms was 0.0001 m. Idle at +0.
  - **Settle.** 1 s after release, avatar (0.000, 15.028) vs snapshot (0.000, 15.028), seq 71: distance 0.000 m, `LastError` 0.000, not snapped.
  - **Bots.** A clean 3 s sample of all 5 bots (result written to a file, no MCP calls during the sample, worst frame 12.1 ms):
    - modes: `Interpolating` 7615, `Extrapolating` 10, `Holding` 0;
    - plan criterion with `unscaledDeltaTime`: 0 jumps, worst ratio 0.96.
    - Against the real `onBeforeRender` interval there were 15 events (3 frames × 5 bots, ratio ≤ 1.12), all `Interpolating` with no clock offset change. There the callback interval (1.8 ms) was shorter than the frame's `unscaledDeltaTime` (3.3 ms): frame pacing in the measurement, not motion.
  - [T4.2-after.png](./screenshots/T4.2-after.png).
- **Caveats.**
  - **Unexplained jumps in the first bot sample.** The first 3 s sample (1 bot, taken right after join) had 3 jumps over the limit, max ratio 2.36. The clean re-sample did not reproduce them, and the cause isn't established. T5.1 re-checks with 149 bots.
  - **MCP polls stall the editor.** A 10 s diagnostic sample with two `execute_code` polls during it had one 883 ms editor stall (CodeDom compiles on the main thread). All remotes underran to `Holding` and caught up with 0.5 m / 0.17 m steps. Samplers must not be polled while they run.
  - **Existing reconnect bug in `NetworkClient` (not changed here).** After the server connection drops without `Disconnect()`, the old `SendLoop` keeps waiting on the shared `sendSignal`. On reconnect it can take the next queued message (here the join) and fail on the disposed socket. The client sat in `Joining` until the join was re-sent. Fresh play sessions are unaffected.
- **Deviation.** `manage_components set_property` and the MCP component resource both threw "Exception has been thrown by the target of an invocation" for ZoneView. The reference was set via `execute_code` (`SerializedObject` + `EditorSceneManager.SaveScene`) and verified in the saved `Demo.unity`.

---

## Phase 5 — Acceptance

**Goal:** Prove every requirement criterion under 200/80 with 150 players, and record it.
**Exit condition:** `docs/demo-run.md` Run 4 has a filled acceptance table with every criterion ✅ (or a named deviation), and the before/after screenshots are linked.

### [ ] T5.1 — Acceptance run under 200/80, Run 4 in `demo-run.md`, wire-example doc update

**Depends on:** T1.1, T4.2
**Files:** `docs/demo-run.md` (Run 4 "After" + acceptance table), `docs/screenshots/prediction-after.png` (new), `docs/TECHNICAL_DESIGN.md` (wire examples at lines ~112-113)

**Context:**
- **The client:** it now predicts the local player (`LocalPredictor`) and interpolates remote players (`SnapshotInterpolator` + `TickClock`), wired in `ZoneView`.
- **The baseline:** T1.1 recorded frame time and key→motion latency under 200/80 in `demo-run.md` Run 4 "Before".
- **Criteria source:** `docs/REQUIREMENTS-prediction.md`.
- **Verification method:** `docs/TECHNICAL_DESIGN-prediction.md` → Testing & verification.

**Do:**
- **Setup:** release build, server `LATENCY_MS=200 JITTER_MS=80`, 149 bots for 120 s, Unity joins as `EditorA`.
- **Start latency:** the same sampler as T1.1. Report frames and ms, plus the animator `Moving` frame.
- **Stop:** release W. Record the stop-frame latency. Log `LocalPredictor.LastError` and `|display − snapshot pos|` for each snapshot for 250 ms after the first snapshot with `seq ≥ stopSeq`.
- **Straight walk:** hold W 5 s, and count `LastCorrectionSnapped == true`. It must be 0.
- **Remotes:**
  - Record 10 bot avatars' positions each frame for 10 s.
  - Keep only segments where consecutive snapshot headings are within 1° and the bot is > 2 m from the ±50 bounds.
  - In those segments, count along-path reversals (must be 0) and frame jumps > `5 × dt × 2` (must be 0).
- **Underrun:** point to the EditMode `Interpolator_Underrun_ExtrapolatesThenHolds` result. Optionally observe `CurrentMode` counts during the run.
- **Frame time:** a 10 s sample with the same method as T1.1. It must be ≥ baseline fps × 0.9.
- **Tests:** `cargo test` from `dumb-server/` and `run_tests` EditMode, with results recorded.
- **Screenshot** mid-walk in the crowd: `Captures/prediction-after.png`, copied to `docs/screenshots/`.
- **Docs:**
  - Write Run 4 "After": results table with before vs after, an acceptance table with one row per `REQUIREMENTS-prediction.md` criterion, caveats, and screenshot links.
  - Update the `docs/TECHNICAL_DESIGN.md` wire examples to the new `joined` / `snapshot` shapes.

**Acceptance:**
- [x] Run 4 acceptance table covers all 9 criteria of `REQUIREMENTS-prediction.md`, each with its evidence and a ✅ or a named deviation.
- [x] Start and stop latency after the change ≤ 1 frame, vs the T1.1 baseline shown side by side.
- [x] 0 snaps on the straight walk, 0 remote reversals or oversized jumps in the filtered segments, and settle ≤ 0.3 m within 250 ms.
- [x] Frame time avg fps ≥ 0.9 × T1.1 baseline.
- [x] `cargo test` and EditMode are green. `prediction-before.png` and `prediction-after.png` are linked from Run 4. `TECHNICAL_DESIGN.md` wire examples include `worldHalf`, `tickHz`, `tick` and `ageMs`.

**Notes (2026-09-15):** Full results and caveats are in `docs/demo-run.md` Run 4 "After".
- **Latency:** start +0 frames, stop +0 frames (baseline +45 / +73).
- **Straight walk:** 0 snaps in 100 reconciles.
- **Settle:** 0.002 m at +250 ms after the ack.
- **Remotes:** 0 reversals and 0 jumps across 11,460 filtered pairs.
- **Frame time:** 142.7 fps vs 153.4 fps (93.0 %).
- **Tests:** `cargo test` 55/55, EditMode 20/20 after leaving play mode.
- **Process:** results came from a hook that wasn't polled while running; server and bots were stopped afterwards.

---

## Open questions
- **Synthetic key timing.** Run 2 noted that key events queued via MCP apply from the editor update callback, not every frame. The ≤ 1 frame criterion counts from the frame `wKey.isPressed` is observed true, not from the MCP call. If polling can't resolve single frames reliably, T1.1 / T4.2 should say so and fall back to `Time.frameCount` stamps captured inside a temporary `MonoBehaviour`. That component is test-only, created via `execute_code` and never saved to the scene.
- **Tick-quantization residual.** Expected ≤ 0.25 m one-time after a direction change (design risk). If T5.1 shows visible pulls, it gets flagged in Run 4 caveats. A server-side fix (integrating from input arrival time) is out of scope.
- **Old clients vs new server.** T2.3 checks that the current client still works on the new server before the wire-in. No backward compatibility is required after that, since client and server ship together.
