# Requirements: Client HUD — FPS & Ping

## Goal
When judging netcode (prediction, interpolation, netsim), there is no on-screen readout of how the client is performing or what round-trip time it is seeing. Add a small HUD to the top-left corner of the Unity client that shows the current FPS and the current network ping (round-trip time).

## Scope
**In scope:**
- A HUD in the top-left corner of the Unity client, visible only while `NetworkClient` is `InWorld`.
- An FPS readout.
- A ping readout derived from the existing `t0` echo in `snapshot` messages. No protocol or server changes.
- The same behavior in the editor and the standalone build (same scene).
- Hiding the join screen's top-left `StatusText` while `InWorld`, since it occupies the HUD's spot (decided during design).
- Unit tests for the ping calculation, plus before/after play-mode screenshots.

**Out of scope:**
- New `ping`/`pong` messages, WebSocket control-frame pings, or any server change.
- Removing the server tick wait from the ping figure (see Outputs).
- Smoothing or averaging ping.
- Color thresholds, icons, graphs, or extra stats (packet loss, jitter, player count).
- A toggle key or settings to hide the HUD.

## Inputs
- **Frame time:** `Time.unscaledDeltaTime`, so the FPS readout ignores `Time.timeScale`.
- **Ping:** each `snapshot` carries the local player's entry with `seq` and `t0`. `t0` is the client's own `DateTimeOffset.UtcNow` ms timestamp from the input with that `seq`. The client and the value share a clock, so this is valid across machines.
- **Input cadence:** `MovementInput` sends input on every direction change and again every 100 ms while in world, even when idle. A new `seq` therefore comes back roughly every 100 ms.
- **Connection state:** `NetworkClient.OnStateChanged`.

## Outputs
- Two lines of text, top-left, no colors that change with the value:
  ```
  FPS: 144
  Ping: 52 ms
  ```
- **FPS:** whole number, the average over the last ~0.5 s (frames counted / elapsed unscaled time), refreshed every ~0.5 s.
- **Ping:** whole milliseconds, the **latest sample** with no smoothing. A sample is taken on the first snapshot whose local-player `seq` is greater than the last sampled `seq`: `ping = now_ms − t0`. Later snapshots repeating the same `seq` do not resample. This avoids counting a stale `t0` as growing latency.
- **What ping includes:** uplink + server tick wait (0–50 ms at 20 Hz) + downlink. It is therefore higher than a pure RTT by up to one tick. This is accepted.
- Before the first sample after joining, ping shows `Ping: -- ms`.
- If a snapshot has no entry for the local player, or its `seq` is not newer, the last value stays on screen.
- The HUD hides on leaving `InWorld` (disconnect, etc.). On the next join, ping resets to `--`.

## Acceptance criteria
- **Given** the client is `Disconnected`, `Connecting` or `Joining`, **when** the join screen shows, **then** the HUD is not visible.
- **Given** the client joined the world, **when** it is rendered, **then** the HUD's two lines are in the top-left corner and do not overlap other UI.
- **Given** the client is in world, **when** FPS is compared with `1 / mean(Time.unscaledDeltaTime)` sampled over the same window, **then** the displayed value is within ±10%.
- **Given** the server with `LATENCY_MS=0`, `JITTER_MS=0` on localhost, **when** the player is in world for 5 s, **then** ping shows a number from 0 to 60 ms, and it updates (it is not stuck at `--`).
- **Given** the server with `LATENCY_MS=100`, `JITTER_MS=0`, **when** the player is in world for 5 s, **then** ping shows a number from 100 to 175 ms.
- **Given** a sequence of snapshots where the local player's `seq` repeats, **when** the ping calculator processes them, **then** it samples only on the first snapshot with each new `seq` (EditMode test).
- **Given** a snapshot without the local player, or before any sample, **when** the ping calculator processes it, **then** there is no value / the previous value stays (EditMode test).
- **Given** the client disconnects and joins again, **when** the HUD reappears, **then** ping shows `--` until the first new sample.
- **Given** the finished feature, **when** it is reviewed, **then** before and after play-mode screenshots are saved under `docs/screenshots/`, and all existing EditMode tests and `cargo test` still pass.

## Constraints & dependencies
- Unity 6000.6.0f1. UI uses uGUI with `UnityEngine.UI.Text` on a Screen Space Overlay canvas, matching `JoinScreen`. No new packages.
- The HUD consumes `NetworkClient` events and does not change the protocol. It stays loosely coupled: no other component depends on it.
- The HUD must not add measurable per-frame cost. At most, text updates when the displayed value changes.
- The 150-players-per-zone requirement is unaffected, since there are no server changes.
- Workflow is set in `CLAUDE.md`: technical design, then taskmaster plan, then one task at a time. No git add/commit/push. Never take control of the mouse or keyboard.

## Open questions
- **Ping during long input gaps:** if input stops being sent (e.g. the app loses focus and `Update` throttles), ping holds its last value rather than growing. Accepted unless it proves misleading.
