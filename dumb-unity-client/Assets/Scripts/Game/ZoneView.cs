using System.Collections.Generic;
using UnityEngine;

namespace Demo
{
    /// <summary>
    /// Owns the avatar registry. Reconciles purely from snapshots: spawns unknown ids,
    /// removes ids missing from the snapshot, and smooths the 20 Hz steps in Update.
    /// </summary>
    public class ZoneView : MonoBehaviour
    {
        [SerializeField] private NetworkClient client;
        [SerializeField] private PlayerAvatar avatarPrefab;
        [SerializeField] private CameraFollow cameraFollow;
        [SerializeField] private float smoothing = 15f;

        private class Entry
        {
            public PlayerAvatar Avatar;
            public Vector3 TargetPosition;
            public Quaternion TargetRotation;
            public string State;
        }

        private readonly Dictionary<long, Entry> entries = new Dictionary<long, Entry>();
        private readonly HashSet<long> seen = new HashSet<long>();
        private readonly List<long> stale = new List<long>();
        private long localPlayerId = -1;

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
        }

        private void HandleJoined(ServerJoined joined) => localPlayerId = joined.playerId;

        private void HandleSnapshot(ServerSnapshot snapshot)
        {
            seen.Clear();
            foreach (var player in snapshot.players)
            {
                seen.Add(player.id);
                var position = new Vector3((float)player.x, 0f, (float)player.z);
                // Server yaw is atan2(dir_z, dir_x); face that direction (model forward is +z).
                var rotation = Quaternion.LookRotation(new Vector3(Mathf.Cos((float)player.yaw), 0f, Mathf.Sin((float)player.yaw)));

                if (!entries.TryGetValue(player.id, out var entry))
                {
                    entry = new Entry { Avatar = Spawn(player, position, rotation) };
                    entries.Add(player.id, entry);
                }

                entry.TargetPosition = position;
                entry.TargetRotation = rotation;
                entry.State = player.state;
            }

            stale.Clear();
            foreach (var id in entries.Keys)
                if (!seen.Contains(id)) stale.Add(id);
            foreach (var id in stale) Remove(id);
        }

        private PlayerAvatar Spawn(ServerPlayer player, Vector3 position, Quaternion rotation)
        {
            var avatar = Instantiate(avatarPrefab, position, rotation, transform);
            var isLocal = player.id == localPlayerId;
            avatar.name = $"Player_{player.id}";
            avatar.SetName(player.name);
            avatar.SetLocal(isLocal);
            if (isLocal) cameraFollow.target = avatar.transform;
            return avatar;
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
        }

        private void Update()
        {
            // Frame-rate independent exponential smoothing toward the latest snapshot.
            var t = 1f - Mathf.Exp(-smoothing * Time.deltaTime);
            foreach (var entry in entries.Values)
            {
                var avatarTransform = entry.Avatar.transform;
                avatarTransform.position = Vector3.Lerp(avatarTransform.position, entry.TargetPosition, t);
                avatarTransform.rotation = Quaternion.Slerp(avatarTransform.rotation, entry.TargetRotation, t);
                entry.Avatar.SetState(entry.State);
            }
        }
    }
}
