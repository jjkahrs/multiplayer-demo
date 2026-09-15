using UnityEngine;

namespace Demo
{
    /// <summary>
    /// The client's copy of the server movement rule (dumb-server/crates/server/src/player.rs integrate).
    /// Positions are Vector2(x, z).
    /// </summary>
    public readonly struct MovementRules
    {
        // Server's MOVE_EPSILON (1e-6) squared: shorter directions count as standing still.
        private const float MoveEpsilonSqr = 1e-12f;

        public readonly float Speed;
        public readonly float WorldHalf;
        public readonly int TickHz;

        public MovementRules(float speed, float worldHalf, int tickHz)
        {
            Speed = speed;
            WorldHalf = worldHalf;
            TickHz = tickHz;
        }

        public MovementRules(ServerJoined joined) : this((float)joined.speed, (float)joined.worldHalf, (int)joined.tickHz) { }

        /// <summary>Mirrors player.rs integrate: direction must be normalized or zero; each axis clamps to ±WorldHalf.</summary>
        public Vector2 Step(Vector2 position, Vector2 direction, float dt)
        {
            if (direction.sqrMagnitude <= MoveEpsilonSqr) return position;
            return new Vector2(
                Mathf.Clamp(position.x + direction.x * Speed * dt, -WorldHalf, WorldHalf),
                Mathf.Clamp(position.y + direction.y * Speed * dt, -WorldHalf, WorldHalf));
        }
    }
}
