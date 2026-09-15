using System;
using System.Collections.Generic;
using UnityEngine;

namespace Demo
{
    // Wire messages mirror dumb-server/crates/protocol/src/messages.rs.
    // Field names are the camelCase wire names because JsonUtility maps fields 1:1.

    [Serializable]
    public class ClientJoin
    {
        public string type = "join";
        public string name;

        public ClientJoin(string name) => this.name = name;
    }

    [Serializable]
    public class ClientInput
    {
        public string type = "input";
        public double vx;
        public double vz;
        public long seq;
        public long t0;
    }

    [Serializable]
    public class ServerJoined
    {
        public string type;
        public long playerId;
        public string name;
        public double x;
        public double z;
        public double yaw;
        // Movement rules for client prediction.
        public double speed;
        public double worldHalf;
        public long tickHz;
    }

    [Serializable]
    public class ServerPlayer
    {
        public long id;
        public string name;
        public double x;
        public double z;
        public double yaw;
        public string state; // "idle" | "walk"
        public long seq;
        public long t0;
        public long ageMs; // how long the server has integrated input seq
    }

    [Serializable]
    public class ServerSnapshot
    {
        public string type;
        public long tick; // zone tick counter, monotonic
        public List<ServerPlayer> players;
    }

    [Serializable]
    public class ServerPlayerJoined
    {
        public string type;
        public long id;
        public string name;
    }

    [Serializable]
    public class ServerPlayerLeft
    {
        public string type;
        public long id;
    }

    [Serializable]
    public class ServerErrorMsg
    {
        public string type;
        public string code;
        public string message;
    }

    public static class Protocol
    {
        [Serializable]
        private class Envelope
        {
            public string type;
        }

        public static string ToJson(object message) => JsonUtility.ToJson(message);

        public static T FromJson<T>(string json) => JsonUtility.FromJson<T>(json);

        /// <summary>Parses a server frame into its typed message, or null if malformed or unknown.</summary>
        public static object ToMessage(string json)
        {
            try
            {
                return FromJson<Envelope>(json)?.type switch
                {
                    "joined" => FromJson<ServerJoined>(json),
                    "snapshot" => FromJson<ServerSnapshot>(json),
                    "playerJoined" => FromJson<ServerPlayerJoined>(json),
                    "playerLeft" => FromJson<ServerPlayerLeft>(json),
                    "error" => FromJson<ServerErrorMsg>(json),
                    _ => null,
                };
            }
            catch (ArgumentException)
            {
                return null;
            }
        }
    }
}
