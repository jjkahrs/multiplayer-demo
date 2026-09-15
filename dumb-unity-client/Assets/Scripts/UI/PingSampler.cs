using System;

namespace Demo
{
    /// <summary>
    /// Round-trip time from the t0 echo: the first snapshot carrying a new local seq gives now − t0.
    /// Includes the server tick wait (up to one tick).
    /// </summary>
    public class PingSampler
    {
        private long playerId = -1;
        // Starts at 0: before any input the server echoes seq 0 / t0 0, which would read as ~1.7e12 ms.
        private long lastSeq;

        /// <summary>Latest sample in ms, or null before the first one.</summary>
        public long? PingMs { get; private set; }

        public void SetLocalPlayer(long id)
        {
            playerId = id;
            lastSeq = 0;
            PingMs = null;
        }

        public void Reset() => SetLocalPlayer(-1);

        /// <summary>Returns true when PingMs changed value.</summary>
        public bool OnSnapshot(ServerSnapshot snapshot, long nowMs)
        {
            if (playerId < 0 || snapshot.players == null) return false;

            var entry = snapshot.players.Find(p => p.id == playerId);
            // Same seq again means a stale t0: it would count waiting time as latency.
            if (entry == null || entry.seq <= lastSeq) return false;

            lastSeq = entry.seq;
            // Clamped: a system clock stepping backwards must not show negative ping.
            long next = Math.Max(0, nowMs - entry.t0);
            var changed = next != PingMs;
            PingMs = next;
            return changed;
        }
    }
}
