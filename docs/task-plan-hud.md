# Task Plan: Client HUD — FPS & Ping

Source design: [TECHNICAL_DESIGN-hud.md](./TECHNICAL_DESIGN-hud.md)
Source requirements: [REQUIREMENTS-hud.md](./REQUIREMENTS-hud.md)

Build/test commands:
- **Unity tests.** MCP `run_tests`, mode EditMode, assembly `Demo.Tests.EditMode`.
- **Server, no simulation.** In PowerShell, from `dumb-server/`: `cargo run --release -p server`. This runs in memory on `0.0.0.0:8080`.
- **Server with 100 ms latency.** In PowerShell, from `dumb-server/`: `$env:LATENCY_MS='100'; $env:JITTER_MS='0'; cargo run --release -p server`. Afterwards: `Remove-Item Env:LATENCY_MS, Env:JITTER_MS`.
- **Server tests.** From `dumb-server/`: `cargo test`.

## How to use this plan
Work tasks in ID order. Each task is self-contained: read its block, do it, check its acceptance criteria, then change its checkbox to `[x]`. Don't start a task until its dependencies are checked off.

Notes that apply across the whole plan:
- **Never** `git add` / `git commit` / `git push` (CLAUDE.md).
- **Unity is agent-driven through Unity MCP.** Never use the OS mouse or keyboard.
  - Scene: `Assets/Scenes/Demo.unity`.
  - Join by setting `NameField` text and invoking `JoinButton.onClick` via `execute_code`. The host field defaults to `ws://127.0.0.1:8080/ws`.
- **Screenshots.** MCP writes only inside the Unity project. Capture to `dumb-unity-client/Captures/<name>.png`, then copy to `docs/screenshots/`.
- **Style.**
  - C#: namespace `Demo`, `[SerializeField] private` fields, subscribe in `OnEnable` and unsubscribe in `OnDisable` (copy `ZoneView.cs`/`JoinScreen.cs`), `///` summaries on public types.
  - Mark deliberate shortcuts with `// ponytail:` comments.
- **No server or protocol changes** in this plan.
- **Parallel-safe:** T1.1 and T1.2 have no dependency on each other.

## Progress
- [x] Phase 1 — HUD logic (EditMode-tested, not wired)
  - [x] T1.1 — `FpsCounter` + tests
  - [x] T1.2 — `PingSampler` + tests
- [x] Phase 2 — Wire-in
  - [x] T2.1 — `Hud` MonoBehaviour, hide `StatusText` in world, `HudText` in scene (before/after screenshots)
- [x] Phase 3 — Acceptance
  - [x] T3.1 — Acceptance run: FPS accuracy, ping with netsim off and at 100 ms, visibility, rejoin

---

## Phase 1 — HUD logic

**Goal:** the FPS and ping calculations exist as plain C# classes, proven by EditMode tests.
**Exit condition:** MCP `run_tests` EditMode passes, including the new `FpsCounterTests` and `PingSamplerTests`. Nothing in the scene has changed yet.

### [x] T1.1 — Add `FpsCounter` and its tests

**Depends on:** none
**Files:** `dumb-unity-client/Assets/Scripts/UI/FpsCounter.cs` (new), `dumb-unity-client/Assets/Tests/EditMode/FpsCounterTests.cs` (new)

**Context:**
- The Unity client has no FPS measurement today.
- Runtime scripts compile into `Assets/Scripts/Demo.asmdef`, and EditMode tests into `Assets/Tests/EditMode/Demo.Tests.EditMode.asmdef`, which already references it. New files need no asmdef change.
- A later task (T2.1) calls this class from a MonoBehaviour every frame with `Time.unscaledDeltaTime`, and rewrites the HUD text only when `Tick` returns true.

**Do:**
- Create `public class FpsCounter` in namespace `Demo`:
  - Constructor `FpsCounter(float windowSeconds = 0.5f)`.
  - `public int Fps { get; private set; }`, which is 0 until the first window completes.
  - `public bool Tick(float unscaledDeltaTime)`:
    - `frames++; elapsed += dt;`
    - When `elapsed >= window`: `next = Mathf.RoundToInt(frames / elapsed)`, then reset `frames` and `elapsed` to 0, set `changed = next != Fps`, set `Fps = next`, and return `changed`.
    - Otherwise return false.
  - `public void Reset()` zeroes `frames`, `elapsed` and `Fps`.
- Use frames/elapsed, not a mean of `1/dt`. Put that reason in one comment line.
- Create `FpsCounterTests` (NUnit, namespace `Demo.Tests`, like `ProtocolTests.cs`) with:
  - `FpsCounter_ConstantDt_ReportsRate`: tick with dt = 1/144 for 1 s, and `Fps == 144`.
  - `FpsCounter_ReportsOnlyAfterWindow`: ticks totalling < 0.5 s all return false and `Fps == 0`. The tick that crosses 0.5 s returns true.
  - `FpsCounter_MixedFrames_UsesFramesOverElapsed`: 9 frames of 0.01 s plus 1 frame of 0.41 s (total 0.5 s, 10 frames) gives `Fps == 20`, not the mean of reciprocals.
  - `FpsCounter_SameRate_ReturnsFalse`: a second window at the same rate returns false.

**Acceptance:**
- [x] MCP `run_tests` EditMode: all 4 `FpsCounterTests` pass.
- [x] All pre-existing EditMode tests still pass.
- [x] `read_console` shows no compile errors.

**Notes:**
- Run result: 24/24 EditMode passed (4 new plus 20 existing).
- The only console errors are Unity AI `NoSubscription` messages, unrelated to this change.
- Test dt values are chosen to stay clear of float rounding at the exact 0.5 s boundary: 0.42 s for the long frame, 0.125 s (exact in binary) for the same-rate test.

---

### [x] T1.2 — Add `PingSampler` and its tests

**Depends on:** none
**Files:** `dumb-unity-client/Assets/Scripts/UI/PingSampler.cs` (new), `dumb-unity-client/Assets/Tests/EditMode/PingSamplerTests.cs` (new)

**Context:**
- Every `snapshot` contains one `ServerPlayer` per player, with `id`, `seq` and `t0` (`Assets/Scripts/Protocol/Protocol.cs`).
- For the local player, `t0` is the client's own `DateTimeOffset.UtcNow.ToUnixTimeMilliseconds()`, stamped when input `seq` was sent (`NetworkClient.SetInput`). So `now − t0` on arrival is the round trip: uplink + server tick wait (0–50 ms) + downlink.
- Input is resent every 100 ms, but snapshots arrive at 20 Hz, so the same `seq` repeats. Only the first arrival of each `seq` is a valid sample.
- Before any input, the server reports `seq 0, t0 0`.
- A later task (T2.1) passes `NetworkClient.OnSnapshot` data and the current Unix ms into this class.

**Do:**
- Create `public class PingSampler` in namespace `Demo`:
  - Private state: `long playerId = -1`, `long lastSeq = 0`.
  - `public long? PingMs { get; private set; }`, where null means no sample yet.
  - `public void SetLocalPlayer(long id)` sets `playerId = id`, `lastSeq = 0` and `PingMs = null`.
  - `public void Reset()` sets `playerId = -1`, `lastSeq = 0` and `PingMs = null`.
  - `public bool OnSnapshot(ServerSnapshot snapshot, long nowMs)`:
    1. Return false if `playerId < 0` or `snapshot.players == null`.
    2. Find the entry with `id == playerId`. Return false if there is none (the last value is kept).
    3. Return false if `entry.seq <= lastSeq`.
    4. Set `lastSeq = entry.seq` and `next = Math.Max(0, nowMs - entry.t0)`. Set `changed = next != PingMs`, then `PingMs = next`, and return `changed`.
- Comment why `lastSeq` starts at 0 (a pre-input `t0 0` would read as ~1.7×10¹² ms) and why the result is clamped at 0 (the system clock can step backwards).
- Create `PingSamplerTests` (NUnit, namespace `Demo.Tests`). Build `ServerSnapshot` objects directly with `players = new List<ServerPlayer> { ... }`. Tests:
  - `PingSampler_BeforeJoin_Null`: no `SetLocalPlayer`, a snapshot containing id 7 → returns false, `PingMs == null`.
  - `PingSampler_SeqZero_NoSample`: `SetLocalPlayer(7)`, entry `seq 0, t0 0` → `PingMs == null`.
  - `PingSampler_NewSeq_Samples`: entry `seq 5, t0 1000`, now 1100 → returns true, `PingMs == 100`.
  - `PingSampler_RepeatedSeq_SamplesOnce`: same seq 5 again at now 1300 → returns false, `PingMs == 100`. Then seq 6 with t0 1200 at now 1350 → `PingMs == 150`.
  - `PingSampler_NoLocalEntry_KeepsLast`: after a sample, a snapshot without id 7 → returns false, value unchanged.
  - `PingSampler_OtherPlayersIgnored`: a snapshot with id 9 at seq 50 doesn't sample for id 7.
  - `PingSampler_NegativeDelta_ClampsToZero`: t0 2000, now 1900 → `PingMs == 0`.
  - `PingSampler_SetLocalPlayer_ClearsPing`: after a sample, `SetLocalPlayer(8)` → `PingMs == null`, and seq 1 samples again.

**Acceptance:**
- [x] MCP `run_tests` EditMode: all 8 `PingSamplerTests` pass.
- [x] Teeth check: temporarily change step 3 to `entry.seq < lastSeq`. `PingSampler_RepeatedSeq_SamplesOnce` must fail. Restore it afterwards and record the result in this task's notes.
- [x] All pre-existing EditMode tests still pass, and `read_console` shows no compile errors.

**Notes:**
- Run result: 32/32 EditMode passed (8 new plus 24 existing).
- Teeth check: with `<`, 3 tests failed, each with `Expected: False But was: True`:
  - `RepeatedSeq_SamplesOnce`;
  - `SeqZero_NoSample`;
  - `OtherPlayersIgnored`, whose local entry is seq 0.
- After restoring `<=`: 32/32 passed again.
- The only console errors are Unity AI `NoSubscription` messages, unrelated to this change.
- `Reset()` is implemented as `SetLocalPlayer(-1)`, which gives the same effect as the design (id −1, `lastSeq` 0, `PingMs` null) with less code.

---

## Phase 2 — Wire-in

**Goal:** the HUD shows in play mode, top-left, only while in world.
**Exit condition:** `docs/screenshots/hud-before.png` shows the pre-change world view with `InWorld` top-left. `docs/screenshots/hud-after.png` shows `FPS: N` / `Ping: N ms` top-left with no `StatusText`.

### [x] T2.1 — Add the `Hud` MonoBehaviour, hide `StatusText` in world, and create `HudText` in the scene

**Depends on:** T1.1, T1.2
**Files:** `dumb-unity-client/Assets/Scripts/UI/Hud.cs` (new), `dumb-unity-client/Assets/Scripts/UI/JoinScreen.cs` (edit `ShowState`), `dumb-unity-client/Assets/Scenes/Demo.unity` (new `HudText` object, `Hud` component on `DemoNet`), `docs/screenshots/hud-before.png` and `hud-after.png` (new)

**Context:**
- **Existing classes.**
  - `FpsCounter` (T1.1): `Tick(float dt)` returns true when `Fps` changes. It also has `Reset()`.
  - `PingSampler` (T1.2): `SetLocalPlayer(long)`, `Reset()`, `OnSnapshot(ServerSnapshot, long nowMs)` returns true when `PingMs` (a `long?`) changes.
- **`NetworkClient`** (`Assets/Scripts/Networking/NetworkClient.cs`, on `DemoNet`):
  - An FSM with states `Disconnected`, `Connecting`, `Joining`, `InWorld`.
  - Raises `OnStateChanged(State)`, `OnJoined(ServerJoined)` (which has `playerId`) and `OnSnapshot(ServerSnapshot)` on the main thread.
- **`JoinCanvas`:** Screen Space Overlay, `CanvasScaler` scale-with-screen at 1920×1080.
- **`StatusText`:** a child of `JoinCanvas` (not `JoinPanel`), anchored top-left at `(20, -20)`, 900×40, 28 px, built-in legacy font. `JoinScreen.ShowState` currently toggles only `panel`, so `StatusText` stays visible in world, showing `InWorld`. The HUD must take that spot.

**Do:**
1. **Before screenshot.** Start the server (no simulation), enter play mode, join as `HudBefore`, capture `hud-before.png`, then exit play mode.
2. **`JoinScreen.ShowState`:** add `statusText.gameObject.SetActive(state != NetworkClient.State.InWorld);`.
3. **Create `public class Hud : MonoBehaviour`** in namespace `Demo`:
   - Fields: `[SerializeField] private NetworkClient client;` and `[SerializeField] private Text text;`.
   - Private members: `readonly FpsCounter fps = new FpsCounter();` and `readonly PingSampler ping = new PingSampler();`.
   - `Awake`: `ShowState(client.CurrentState)`.
   - `OnEnable` / `OnDisable`: subscribe and unsubscribe `client.OnJoined`, `client.OnSnapshot` and `client.OnStateChanged`.
   - `HandleJoined(joined)`: `ping.SetLocalPlayer(joined.playerId); fps.Reset(); Render();`.
   - `HandleSnapshot(s)`: `if (ping.OnSnapshot(s, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds())) Render();`.
   - `ShowState(state)`: `text.gameObject.SetActive(state == InWorld)`. If the state is not `InWorld`, call `ping.Reset()`.
   - `Update`: `if (text.gameObject.activeSelf && fps.Tick(Time.unscaledDeltaTime)) Render();`.
   - `Render`: `text.text = $"FPS: {fps.Fps}\nPing: {ping.PingMs?.ToString() ?? "--"} ms";`.
4. **Scene, via MCP** (`manage_gameobject` / `manage_components`, or `execute_code` plus a scene save):
   - Create `HudText` as a child of `JoinCanvas`, with a `Text` component:
     - built-in legacy font (same as `StatusText`), 28 px, white, `UpperLeft` alignment;
     - `raycastTarget = false`;
     - horizontal and vertical overflow set to `Overflow`.
   - `HudText` RectTransform: anchorMin, anchorMax and pivot `(0, 1)`, anchoredPosition `(20, -20)`, sizeDelta `(400, 80)`.
   - Add `Hud` to `DemoNet`, and set `client` to `DemoNet`'s `NetworkClient` and `text` to `HudText`.
   - Save `Demo.unity`.
5. **After screenshot.** Enter play mode, join as `HudAfter`, wait 2 s, capture `hud-after.png`.

**Acceptance:**
- [x] MCP `run_tests` EditMode: all tests pass, and `read_console` shows no errors on entering play mode.
- [x] Before joining: `execute_code` reads `HudText.activeSelf == false` and `StatusText.activeSelf == true`.
- [x] 2 s after joining:
  - `HudText.activeSelf == true` and `StatusText.activeSelf == false`;
  - `HudText.text` matches `^FPS: \d+\nPing: (\d+|--) ms$`, and the FPS number is > 0.
- [x] `docs/screenshots/hud-before.png` shows `InWorld` top-left. `docs/screenshots/hud-after.png` shows the two HUD lines top-left, with nothing overlapping.
- [x] `Demo.unity` is saved: after reopening the scene, `DemoNet` has a `Hud` component whose `client` and `text` references are non-null.

**Notes:**
- **EditMode:** 32/32 passed after the wire-in.
- **Console:** 0 errors on entering play mode (cleared first).
- **Before join:** `Disconnected | HudText active=False | StatusText active=True`.
- **3 s after joining as `HudAfter`:** `InWorld | HudText active=True | StatusText active=False | text='FPS: 573\nPing: 46 ms'`, regex matched. The screenshot a moment later shows `FPS: 605 / Ping: 50 ms`, so the values update.
- **Scene built** with `execute_code` in edit mode:
  - `HudText` under `JoinCanvas`, using the same `LegacyRuntime` font as `StatusText`;
  - `Hud` added to `DemoNet`, references set through `SerializedObject`;
  - scene saved (`SaveScene` returned true).
- **Saved-reference check:** read from the `Demo.unity` YAML instead of reopening the scene. The `Hud` block has `client: {fileID: 1850309257}` and `text: {fileID: 1459407476}`, and the latter is one of `HudText`'s components.
- **Heads-up for T3.1:** localhost ping reads 46–50 ms, near the top of the 0–60 ms range. That fits the up-to-50 ms tick wait, but the 60 ms ceiling leaves little room for frame-time and scheduling noise.

---

## Phase 3 — Acceptance

**Goal:** prove every criterion in `REQUIREMENTS-hud.md` in the running client.
**Exit condition:** every check below is recorded as PASS or FAIL, with numbers, in a new "HUD" section of `docs/demo-run.md`, and `docs/screenshots/hud-after-latency100.png` exists.

### [x] T3.1 — Acceptance run: FPS accuracy, ping off and at 100 ms, visibility, rejoin

**Depends on:** T2.1
**Files:** `docs/demo-run.md` (append a "HUD" section), `docs/screenshots/hud-after-latency100.png` (new)

**Context:**
- **HUD in the scene** (T2.1): `HudText` (child of `JoinCanvas`) shows `FPS: N\nPing: N ms` while `NetworkClient` is `InWorld` and is inactive otherwise. The `Hud` component on `DemoNet` drives it.
- **FPS** is frames/elapsed over 0.5 s windows of `Time.unscaledDeltaTime`.
- **Ping** is `now − t0` sampled on each new local `seq`. It includes a 0–50 ms server tick wait.
- **Netsim** adds `LATENCY_MS` to the round trip (half each way) on data frames.
- **Earlier FPS method:** `docs/demo-run.md` Run 2 sampled frame times with an `Application.onBeforeRender` hook created in a single `execute_code` call. Reuse that approach.

**Do:**
1. **Netsim off.** Start the server with no simulation. Enter play mode and join as `HudAcc`.
2. **FPS check.** In one `execute_code` call:
   - hook `Application.onBeforeRender` to record `Time.unscaledDeltaTime` per frame for 5 s;
   - whenever the `HudText` FPS number changes, record the frame index;
   - afterwards, for each HUD refresh, compute `frames / sum(dt)` over the frames of the preceding 0.5 s window and compare with the displayed value.
   - Log the max % deviation.
3. **Ping off.** Sample `HudText.text` every 0.5 s for 5 s. Discard the first 1 s. Log all values.
4. **Rejoin.**
   - Stop the server, and confirm `HudText.activeSelf == false` and `StatusText.activeSelf == true`.
   - Start the server with `LATENCY_MS=100`, `JITTER_MS=0` and rejoin.
   - Read `HudText.text` within the first frames after `InWorld`, and log whether it shows `Ping: -- ms` or a fresh value.
5. **Ping at 100.** Sample as in step 3. Capture `hud-after-latency100.png`.
6. **Cleanup.** Exit play mode, stop the server, run `Remove-Item Env:LATENCY_MS, Env:JITTER_MS`, then `cargo test` from `dumb-server/`.
7. **Record.** Write the numbers and PASS/FAIL per criterion in `docs/demo-run.md` → "HUD". If a check fails, record FAIL, don't tune thresholds, and flag it to the user.

**Acceptance:**
- [x] FPS: every HUD value is within ±10% of the frame-time-derived rate for its window.
- [x] Ping, netsim off: every sample after 1 s is a number from 0 to 60, and at least two distinct values were seen (it updates).
- [x] Ping, `LATENCY_MS=100`: every sample after 1 s is from 100 to 175. `hud-after-latency100.png` shows it.
- [x] Visibility and rejoin: the HUD is inactive and `StatusText` active while disconnected. After rejoin, the first read shows `--` or a value sampled after the rejoin (never a pre-disconnect value).
- [x] `cargo test` passes (server untouched), and `docs/demo-run.md` has the HUD section with all results.

**Notes:** full numbers are in `docs/demo-run.md` → Run 5.
- **FPS:** 9 comparisons over 3,429 frames, max deviation 0.06%.
- **Ping, netsim off:** 58–7 ms, 11 samples.
- **Ping at 100 ms:** 138–166 ms, 11 samples.
- **Rejoin:** first read was `FPS: 0 / Ping: -- ms`.
- **`cargo test`:** 55 passed.
- **Existing `NetworkClient` reconnect bug hit during the rejoin (not fixed, out of scope).**
  - After the server was killed, the first rejoin sat in `Joining`. Reflection showed `sendQueue` empty and `sendSignal` at 0 with an established socket, and the server reported `players=0`. A stale `SendLoop` had swallowed the join.
  - This matches `demo-run.md:427` and `task-plan-prediction.md:422`.
  - Workaround, same as before: `client.Join("HudLat")` re-sent through `execute_code`.
- **Screenshot miss:** the first `hud-after-latency100` capture, taken while stuck in `Joining`, showed no overlay UI at all (not even the join panel), so it was discarded. The retake after the successful join shows the HUD.
- **Unity console:** 5× "PlayerLoop internal function has been called recursively" appeared in this play session. The cause was not investigated. The HUD kept working.

---

## Open questions
- **Before screenshot moved into T2.1.** The design listed it as a separate first step. It sits at the start of T2.1 so the before/after pair belongs to the task that changes the view. Phase 1 is logic-only and proven by test output, not screenshots, since nothing visible changes.
- **Editor-throttled FPS.** If the editor Game view is unfocused, Unity may throttle frames. The FPS check compares the HUD against the same process's frame times, so it still holds. Absolute FPS numbers from this run aren't comparable to Run 2.
