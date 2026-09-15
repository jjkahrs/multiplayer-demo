using System;
using UnityEngine;
using UnityEngine.InputSystem;

namespace Demo
{
    /// <summary>
    /// Polls WASD and forwards the normalized direction to the server:
    /// immediately when it changes (including release) and every resendInterval while in world.
    /// </summary>
    [DefaultExecutionOrder(-10)] // send before ZoneView advances prediction in the same frame
    public class MovementInput : MonoBehaviour
    {
        [SerializeField] private NetworkClient client;
        [SerializeField] private float resendInterval = 0.1f;

        /// <summary>(seq, direction) of each input sent, raised right after the send.</summary>
        public event Action<long, Vector2> OnInputSent;

        private Vector2 lastSent;
        private float nextSendTime;

        private void Update()
        {
            if (client.CurrentState != NetworkClient.State.InWorld) return;

            var direction = ReadDirection();
            if (direction == lastSent && Time.unscaledTime < nextSendTime) return;

            var seq = client.SetInput(direction.x, direction.y);
            lastSent = direction;
            nextSendTime = Time.unscaledTime + resendInterval;
            if (seq >= 0) OnInputSent?.Invoke(seq, direction);
        }

        private static Vector2 ReadDirection()
        {
            var keyboard = Keyboard.current;
            if (keyboard == null) return Vector2.zero;

            var direction = Vector2.zero;
            if (keyboard.wKey.isPressed) direction.y += 1f;
            if (keyboard.sKey.isPressed) direction.y -= 1f;
            if (keyboard.dKey.isPressed) direction.x += 1f;
            if (keyboard.aKey.isPressed) direction.x -= 1f;
            return direction.normalized;
        }
    }
}
