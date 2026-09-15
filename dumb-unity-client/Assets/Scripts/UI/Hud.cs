using System;
using UnityEngine;
using UnityEngine.UI;

namespace Demo
{
    /// <summary>
    /// Top-left FPS and ping readout, shown only while in world. Text is rewritten only when a value changes.
    /// </summary>
    public class Hud : MonoBehaviour
    {
        [SerializeField] private NetworkClient client;
        [SerializeField] private Text text;

        private readonly FpsCounter fps = new FpsCounter();
        private readonly PingSampler ping = new PingSampler();

        private void Awake() => ShowState(client.CurrentState);

        private void OnEnable()
        {
            client.OnJoined += HandleJoined;
            client.OnSnapshot += HandleSnapshot;
            client.OnStateChanged += ShowState;
        }

        private void OnDisable()
        {
            client.OnJoined -= HandleJoined;
            client.OnSnapshot -= HandleSnapshot;
            client.OnStateChanged -= ShowState;
        }

        private void HandleJoined(ServerJoined joined)
        {
            ping.SetLocalPlayer(joined.playerId);
            fps.Reset();
            Render();
        }

        private void HandleSnapshot(ServerSnapshot snapshot)
        {
            if (ping.OnSnapshot(snapshot, DateTimeOffset.UtcNow.ToUnixTimeMilliseconds())) Render();
        }

        private void ShowState(NetworkClient.State state)
        {
            var inWorld = state == NetworkClient.State.InWorld;
            text.gameObject.SetActive(inWorld);
            if (!inWorld) ping.Reset();
        }

        private void Update()
        {
            if (text.gameObject.activeSelf && fps.Tick(Time.unscaledDeltaTime)) Render();
        }

        private void Render() => text.text = $"FPS: {fps.Fps}\nPing: {ping.PingMs?.ToString() ?? "--"} ms";
    }
}
