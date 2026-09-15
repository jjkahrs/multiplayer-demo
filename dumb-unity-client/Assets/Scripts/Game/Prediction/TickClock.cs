using System.Collections.Generic;

namespace Demo
{
    /// <summary>
    /// Maps server ticks onto local time for rendering remotes. Keeps a sliding-window minimum of
    /// arrival − tick/tickHz: jitter and the queue clamp only ever add delay, so the smallest offset
    /// is the least-delayed path.
    /// </summary>
    public class TickClock
    {
        private struct Sample
        {
            public double Arrival;
            public double Offset;
        }

        private readonly int tickHz;
        private readonly double windowSeconds;
        // Monotonic deque: offsets increase front to back, so the front is the window minimum.
        private readonly LinkedList<Sample> samples = new LinkedList<Sample>();

        public TickClock(int tickHz, double windowSeconds = 2.0)
        {
            this.tickHz = tickHz;
            this.windowSeconds = windowSeconds;
        }

        public bool HasSync => samples.Count > 0;

        /// <param name="arrivalTime">Local receive time in seconds (Time.realtimeSinceStartupAsDouble).</param>
        public void OnSnapshot(long tick, double arrivalTime)
        {
            var offset = arrivalTime - (double)tick / tickHz;
            while (samples.Count > 0 && samples.Last.Value.Offset >= offset) samples.RemoveLast();
            samples.AddLast(new Sample { Arrival = arrivalTime, Offset = offset });
            while (samples.First.Value.Arrival < arrivalTime - windowSeconds) samples.RemoveFirst();
        }

        /// <summary>Fractional server tick to render at local time now. Requires HasSync.</summary>
        public double RenderTick(double now, double renderDelaySeconds) =>
            (now - samples.First.Value.Offset - renderDelaySeconds) * tickHz;
    }
}
