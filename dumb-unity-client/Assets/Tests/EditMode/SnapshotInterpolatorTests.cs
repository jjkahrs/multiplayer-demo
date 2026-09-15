using System;
using System.Collections.Generic;
using NUnit.Framework;
using UnityEngine;

namespace Demo.Tests
{
    public class SnapshotInterpolatorTests
    {
        private const int TickHz = 20;
        private const double TickDt = 1.0 / TickHz;

        private static double Offset(TickClock clock, double now) => now - clock.RenderTick(now, 0) / TickHz;

        [Test]
        public void TickClock_MinOffsetIgnoresJitter()
        {
            const double baseOffset = 10.0;
            var rng = new System.Random(1);
            var clock = new TickClock(TickHz);
            var history = new List<(double arrival, double offset)>();

            for (long tick = 0; tick < 200; tick++)
            {
                var arrival = baseOffset + tick * TickDt + rng.NextDouble() * 0.04;
                clock.OnSnapshot(tick, arrival);
                history.Add((arrival, arrival - tick * TickDt));

                // Brute-force minimum over the same 2 s window.
                var expected = double.MaxValue;
                foreach (var (a, o) in history)
                    if (a >= arrival - 2.0) expected = Math.Min(expected, o);
                Assert.AreEqual(expected, Offset(clock, arrival), 1e-9, $"tick {tick}");
                Assert.AreEqual(baseOffset, Offset(clock, arrival), 0.04 + 1e-9);
            }
        }

        [Test]
        public void TickClock_WindowEviction()
        {
            var clock = new TickClock(TickHz);
            clock.OnSnapshot(0, 10.0); // low outlier: offset 10.00
            for (long tick = 1; tick <= 30; tick++) clock.OnSnapshot(tick, 10.03 + tick * TickDt);
            Assert.AreEqual(10.0, Offset(clock, 11.6), 1e-9, "still inside the window");

            for (long tick = 31; tick <= 60; tick++) clock.OnSnapshot(tick, 10.03 + tick * TickDt);
            Assert.AreEqual(10.03, Offset(clock, 13.1), 1e-9, "outlier aged out");
        }

        [Test]
        public void Interpolator_JitteredArrivals_Monotonic()
        {
            const float speed = 5f;
            const float frameDt = 1f / 144f;
            var rng = new System.Random(7);
            var arrivals = new List<double>();
            var last = 0.0;
            for (var tick = 0; tick < 130; tick++)
            {
                // Latency plus 0-40 ms jitter; a late frame holds back the ones behind it (no overtaking).
                last = Math.Max(tick * TickDt + 0.1 + rng.NextDouble() * 0.04, last);
                arrivals.Add(last);
            }

            var clock = new TickClock(TickHz);
            var interpolator = new SnapshotInterpolator();
            var next = 0;
            var lastX = float.NaN;
            var checkedFrames = 0;
            for (var frame = 0; frame < 5 * 144; frame++)
            {
                var now = frame / 144.0;
                for (; next < arrivals.Count && arrivals[next] <= now; next++)
                {
                    clock.OnSnapshot(next, now);
                    interpolator.Add(next, new Vector2(next * speed * (float)TickDt, 0f), 0f, true);
                }
                if (!clock.HasSync) continue;

                interpolator.Sample(clock.RenderTick(now, 0.1), out var position, out _, out _);
                // Skip the first second: until the window holds a low-delay sample, a new minimum can pull render time forward.
                if (now >= 1.0 && !float.IsNaN(lastX))
                {
                    Assert.GreaterOrEqual(position.x, lastX - 1e-4f, $"moved backward at t={now:F3}");
                    Assert.LessOrEqual(position.x - lastX, speed * frameDt * 2f, $"jumped at t={now:F3}");
                    checkedFrames++;
                }
                lastX = position.x;
            }
            Assert.Greater(checkedFrames, 500);
        }

        [Test]
        public void Interpolator_Underrun_ExtrapolatesThenHolds()
        {
            var interpolator = new SnapshotInterpolator(0.05f, TickHz);
            for (var tick = 0; tick <= 10; tick++) interpolator.Add(tick, new Vector2(tick * 0.25f, 0f), 0f, true);

            interpolator.Sample(9.5, out var position, out _, out var walking);
            Assert.AreEqual(SnapshotInterpolator.Mode.Interpolating, interpolator.CurrentMode);
            Assert.AreEqual(2.375f, position.x, 1e-4f);
            Assert.IsTrue(walking);

            interpolator.Sample(10 + 0.025 * TickHz, out position, out _, out walking);
            Assert.AreEqual(SnapshotInterpolator.Mode.Extrapolating, interpolator.CurrentMode);
            Assert.AreEqual(2.5f + 5f * 0.025f, position.x, 1e-4f);
            Assert.IsTrue(walking);

            interpolator.Sample(10 + 0.08 * TickHz, out position, out _, out walking);
            Assert.AreEqual(SnapshotInterpolator.Mode.Holding, interpolator.CurrentMode);
            Assert.AreEqual(2.5f + 5f * 0.05f, position.x, 1e-4f, "held at newest + v × 0.05");
            Assert.IsFalse(walking);
        }

        [Test]
        public void Interpolator_YawShortestPath()
        {
            var interpolator = new SnapshotInterpolator();
            interpolator.Add(0, Vector2.zero, Mathf.PI - 0.1f, true);
            interpolator.Add(1, Vector2.zero, -Mathf.PI + 0.1f, true);

            interpolator.Sample(0.5, out _, out var yaw, out _);

            Assert.AreEqual(Mathf.PI, Mathf.Abs(yaw), 1e-3f, $"yaw {yaw}");
        }
    }
}
