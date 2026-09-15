using System;
using System.Collections.Concurrent;
using System.IO;
using System.Net.WebSockets;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using UnityEngine;

namespace Demo
{
    /// <summary>
    /// WebSocket driver. Socket I/O runs on a background Task; everything user-visible
    /// (state changes, events) is marshalled to the main thread and raised in Update.
    /// </summary>
    public class NetworkClient : MonoBehaviour
    {
        public enum State { Disconnected, Connecting, Joining, InWorld }

        [SerializeField] private bool logMessages = true;

        public State CurrentState { get; private set; } = State.Disconnected;

        public event Action<State> OnStateChanged;
        public event Action<ServerJoined> OnJoined;
        public event Action<ServerSnapshot> OnSnapshot;
        public event Action<ServerPlayerJoined> OnPlayerJoined;
        public event Action<ServerPlayerLeft> OnPlayerLeft;
        public event Action<ServerErrorMsg> OnError;
        /// <summary>Raised with the failure reason, or null for a local Disconnect().</summary>
        public event Action<string> OnDisconnected;

        private readonly ConcurrentQueue<Action> mainThread = new ConcurrentQueue<Action>();
        private readonly ConcurrentQueue<string> sendQueue = new ConcurrentQueue<string>();
        private readonly SemaphoreSlim sendSignal = new SemaphoreSlim(0);
        private CancellationTokenSource cts;
        private long seq;

        public void Connect(string url)
        {
            if (CurrentState != State.Disconnected) return;
            while (sendQueue.TryDequeue(out _)) { }
            seq = 0;
            cts = new CancellationTokenSource();
            SetState(State.Connecting);
            _ = Run(url, cts.Token);
        }

        /// <summary>Queues a join; sent once the socket opens. Resending while Joining retries after a bad_name.</summary>
        public void Join(string name)
        {
            if (CurrentState != State.Connecting && CurrentState != State.Joining) return;
            Send(new ClientJoin(name));
        }

        public void SetInput(float vx, float vz)
        {
            if (CurrentState != State.InWorld) return;
            Send(new ClientInput
            {
                vx = vx,
                vz = vz,
                seq = ++seq,
                t0 = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
            });
        }

        public void Disconnect() => cts?.Cancel();

        private void Update()
        {
            while (mainThread.TryDequeue(out var action)) action();
        }

        private void OnDestroy() => Disconnect();

        private void Send(object message)
        {
            sendQueue.Enqueue(Protocol.ToJson(message));
            sendSignal.Release();
        }

        private void SetState(State next)
        {
            if (CurrentState == next) return;
            CurrentState = next;
            OnStateChanged?.Invoke(next);
        }

        private async Task Run(string url, CancellationToken ct)
        {
            string reason = null;
            using (var ws = new ClientWebSocket())
            {
                try
                {
                    await ws.ConnectAsync(new Uri(url), ct);
                    mainThread.Enqueue(() => SetState(State.Joining));
                    var finished = await Task.WhenAny(SendLoop(ws, ct), ReceiveLoop(ws, ct));
                    await finished; // rethrow the failure, if any
                    reason = "server closed the connection";
                }
                catch (OperationCanceledException) when (ct.IsCancellationRequested)
                {
                    // Local Disconnect(): reason stays null.
                }
                catch (Exception e)
                {
                    reason = e.Message;
                }
            }

            mainThread.Enqueue(() =>
            {
                if (reason != null) Debug.LogWarning($"[net] disconnected: {reason}");
                SetState(State.Disconnected);
                OnDisconnected?.Invoke(reason);
            });
        }

        private async Task SendLoop(ClientWebSocket ws, CancellationToken ct)
        {
            while (true)
            {
                await sendSignal.WaitAsync(ct);
                while (sendQueue.TryDequeue(out var json))
                {
                    var bytes = Encoding.UTF8.GetBytes(json);
                    await ws.SendAsync(new ArraySegment<byte>(bytes), WebSocketMessageType.Text, true, ct);
                }
            }
        }

        private async Task ReceiveLoop(ClientWebSocket ws, CancellationToken ct)
        {
            var buffer = new byte[64 * 1024];
            using (var frame = new MemoryStream())
            {
                while (true)
                {
                    var result = await ws.ReceiveAsync(new ArraySegment<byte>(buffer), ct);
                    if (result.MessageType == WebSocketMessageType.Close) return;

                    frame.Write(buffer, 0, result.Count);
                    if (!result.EndOfMessage) continue;

                    var json = Encoding.UTF8.GetString(frame.GetBuffer(), 0, (int)frame.Length);
                    frame.SetLength(0);
                    var message = Protocol.ToMessage(json);
                    mainThread.Enqueue(() => Dispatch(message, json));
                }
            }
        }

        private void Dispatch(object message, string json)
        {
            switch (message)
            {
                case ServerJoined joined when CurrentState == State.Joining:
                    Log($"joined playerId={joined.playerId} name={joined.name} pos=({joined.x:F2},{joined.z:F2})");
                    SetState(State.InWorld);
                    OnJoined?.Invoke(joined);
                    break;
                case ServerSnapshot snapshot:
                    Log($"snapshot players={snapshot.players?.Count ?? 0}");
                    OnSnapshot?.Invoke(snapshot);
                    break;
                case ServerPlayerJoined playerJoined:
                    Log($"playerJoined id={playerJoined.id} name={playerJoined.name}");
                    OnPlayerJoined?.Invoke(playerJoined);
                    break;
                case ServerPlayerLeft playerLeft:
                    Log($"playerLeft id={playerLeft.id}");
                    OnPlayerLeft?.Invoke(playerLeft);
                    break;
                case ServerErrorMsg error:
                    Debug.LogWarning($"[net] error {error.code}: {error.message}");
                    OnError?.Invoke(error);
                    break;
                default:
                    Debug.LogWarning($"[net] ignored frame in state {CurrentState}: {json}");
                    break;
            }
        }

        private void Log(string line)
        {
            if (logMessages) Debug.Log($"[net] {line}");
        }
    }
}
