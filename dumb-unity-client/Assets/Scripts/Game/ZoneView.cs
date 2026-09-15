using System.Collections.Generic;
using UnityEngine;

namespace Demo
{
    /// <summary>
    /// Owns the avatar registry: spawns unknown ids and removes ids missing from a snapshot.
    /// The local avatar follows LocalPredictor; remote avatars render renderDelay behind the server
    /// timeline through one SnapshotInterpolator each, all sharing one TickClock.
    /// </summary>
    public class ZoneView : MonoBehaviour
    {
        [SerializeField] private NetworkClient client;
        [SerializeField] private MovementInput movementInput;
        [SerializeField] private PlayerAvatar avatarPrefab;
        [SerializeField] private CameraFollow cameraFollow;
        [SerializeField] private float renderDelay = 0.1f;
        [SerializeField] private float maxExtrapolation = 0.05f;
        [SerializeField] private float correctionTime = 0.1f;
        [SerializeField] private float snapDistance = 1f;

        private class Entry
        {
            public PlayerAvatar Avatar;
            /// <summary>Null for the local player, which follows the predictor.</summary>
            public SnapshotInterpolator Interpolator;
        }

        private readonly Dictionary<long, Entry> entries = new Dictionary<long, Entry>();
        private readonly HashSet<long> seen = new HashSet<long>();
        private readonly List<long> stale = new List<long>();
        private long localPlayerId = -1;
        private MovementRules rules;
        private TickClock clock;
        private LocalPredictor predictor;

        public int AvatarCount => entries.Count;

        private void OnEnable()
        {
            client.OnJoined += HandleJoined;
            client.OnSnapshot += HandleSnapshot;
            client.OnDisconnected += HandleDisconnected;
        }

        private void OnDisable()
        {
            client.OnJoined -= HandleJoined;
            client.OnSnapshot -= HandleSnapshot;
            client.OnDisconnected -= HandleDisconnected;
            StopPrediction();
        }

        private void HandleJoined(ServerJoined joined)
        {
            StopPrediction();
            localPlayerId = joined.playerId;
            rules = new MovementRules(joined);
            clock = new TickClock(rules.TickHz);
            predictor = new LocalPredictor(rules, new Vector2((float)joined.x, (float)joined.z), (float)joined.yaw, correctionTime, snapDistance);
            movementInput.OnInputSent += HandleInputSent;
        }

        private void HandleInputSent(long seq, Vector2 direction) => predictor.SetInput(seq, direction);

        private void HandleSnapshot(ServerSnapshot snapshot)
        {
            // Snapshots can beat the joined reply; without the rules and local id they can't be placed.
            if (clock == null) return;
            clock.OnSnapshot(snapshot.tick, Time.realtimeSinceStartupAsDouble);

            seen.Clear();
            foreach (var player in snapshot.players)
            {
                seen.Add(player.id);
                var position = new Vector2((float)player.x, (float)player.z);
                if (!entries.TryGetValue(player.id, out var entry))
                {
                    entry = Spawn(player, position);
                    entries.Add(player.id, entry);
                }

                if (entry.Interpolator == null) predictor.Reconcile(player.seq, player.ageMs, position);
                else entry.Interpolator.Add(snapshot.tick, position, (float)player.yaw, player.state == "walk");
            }

            stale.Clear();
            foreach (var id in entries.Keys)
                if (!seen.Contains(id)) stale.Add(id);
            foreach (var id in stale) Remove(id);
        }

        private Entry Spawn(ServerPlayer player, Vector2 position)
        {
            var avatar = Instantiate(avatarPrefab, ToWorld(position), ToRotation((float)player.yaw), transform);
            var isLocal = player.id == localPlayerId;
            avatar.name = $"Player_{player.id}";
            avatar.SetName(player.name);
            avatar.SetLocal(isLocal);
            if (isLocal) cameraFollow.target = avatar.transform;
            return new Entry
            {
                Avatar = avatar,
                Interpolator = isLocal ? null : new SnapshotInterpolator(maxExtrapolation, rules.TickHz),
            };
        }

        private void Remove(long id)
        {
            Destroy(entries[id].Avatar.gameObject);
            entries.Remove(id);
        }

        private void HandleDisconnected(string reason)
        {
            foreach (var entry in entries.Values) Destroy(entry.Avatar.gameObject);
            entries.Clear();
            localPlayerId = -1;
            cameraFollow.target = null;
            StopPrediction();
        }

        private void StopPrediction()
        {
            if (movementInput != null) movementInput.OnInputSent -= HandleInputSent;
            predictor = null;
            clock = null;
        }

        private void Update()
        {
            if (predictor == null) return;
            predictor.Advance(Time.deltaTime);
            // Entries only exist after a snapshot, so the clock is synced whenever there is something to place.
            if (entries.Count == 0) return;

            var renderTick = clock.RenderTick(Time.realtimeSinceStartupAsDouble, renderDelay);
            foreach (var entry in entries.Values)
            {
                if (entry.Interpolator == null)
                {
                    Apply(entry.Avatar, predictor.DisplayPosition, predictor.Yaw, predictor.IsMoving);
                    continue;
                }
                entry.Interpolator.Sample(renderTick, out var position, out var yaw, out var walking);
                Apply(entry.Avatar, position, yaw, walking);
            }
        }

        private static void Apply(PlayerAvatar avatar, Vector2 position, float yaw, bool walking)
        {
            avatar.transform.SetPositionAndRotation(ToWorld(position), ToRotation(yaw));
            avatar.SetState(walking ? "walk" : "idle");
        }

        private static Vector3 ToWorld(Vector2 position) => new Vector3(position.x, 0f, position.y);

        // Server yaw is atan2(dir_z, dir_x); face that direction (model forward is +z).
        private static Quaternion ToRotation(float yaw) => Quaternion.LookRotation(new Vector3(Mathf.Cos(yaw), 0f, Mathf.Sin(yaw)));
    }
}
