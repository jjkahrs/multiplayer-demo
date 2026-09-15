using System.Collections.Generic;
using UnityEngine;

namespace Demo
{
    /// <summary>
    /// One remote player's snapshot buffer. Renders between buffered samples; on underrun extrapolates
    /// at the last velocity up to maxExtrapolation, then holds. The mode is derived only from the render
    /// tick against the buffer, inside Sample, so no invalid state combination exists.
    /// </summary>
    public class SnapshotInterpolator
    {
        public enum Mode { Interpolating, Extrapolating, Holding }

        private const int MaxSamples = 32;

        private struct Snapshot
        {
            public long Tick;
            public Vector2 Position;
            public float Yaw;
            public bool Walking;
        }

        private readonly float maxExtrapolationSeconds;
        private readonly int tickHz;
        private readonly List<Snapshot> samples = new List<Snapshot>();

        public SnapshotInterpolator(float maxExtrapolationSeconds = 0.05f, int tickHz = 20)
        {
            this.maxExtrapolationSeconds = maxExtrapolationSeconds;
            this.tickHz = tickHz;
        }

        public Mode CurrentMode { get; private set; } = Mode.Holding;

        /// <summary>Buffers a snapshot; ticks at or before the newest one are ignored.</summary>
        public void Add(long tick, Vector2 position, float yaw, bool walking)
        {
            if (samples.Count > 0 && tick <= samples[samples.Count - 1].Tick) return;
            if (samples.Count == MaxSamples) samples.RemoveAt(0);
            samples.Add(new Snapshot { Tick = tick, Position = position, Yaw = yaw, Walking = walking });
        }

        /// <summary>Requires at least one Add.</summary>
        public void Sample(double renderTick, out Vector2 position, out float yaw, out bool walking)
        {
            var oldest = samples[0];
            if (samples.Count == 1 || renderTick <= oldest.Tick)
            {
                CurrentMode = Mode.Holding;
                position = oldest.Position;
                yaw = oldest.Yaw;
                walking = oldest.Walking;
                return;
            }

            var newest = samples[samples.Count - 1];
            if (renderTick <= newest.Tick)
            {
                var b = 1;
                while (samples[b].Tick < renderTick) b++;
                var from = samples[b - 1];
                var to = samples[b];
                var t = (float)((renderTick - from.Tick) / (to.Tick - from.Tick));
                CurrentMode = Mode.Interpolating;
                position = Vector2.Lerp(from.Position, to.Position, t);
                yaw = from.Yaw + Mathf.DeltaAngle(from.Yaw * Mathf.Rad2Deg, to.Yaw * Mathf.Rad2Deg) * Mathf.Deg2Rad * t;
                walking = from.Walking;
                samples.RemoveRange(0, b - 1);
                return;
            }

            var previous = samples[samples.Count - 2];
            var velocity = (newest.Position - previous.Position) * tickHz / (newest.Tick - previous.Tick);
            var overshoot = (float)((renderTick - newest.Tick) / tickHz);
            yaw = newest.Yaw;
            if (overshoot <= maxExtrapolationSeconds)
            {
                CurrentMode = Mode.Extrapolating;
                position = newest.Position + velocity * overshoot;
                walking = newest.Walking;
            }
            else
            {
                // Frozen at the extrapolation limit, not snapped back to newest.
                CurrentMode = Mode.Holding;
                position = newest.Position + velocity * maxExtrapolationSeconds;
                walking = false;
            }
            samples.RemoveRange(0, samples.Count - 2);
        }
    }
}
