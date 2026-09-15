using UnityEngine;
using UnityEngine.InputSystem;

namespace Demo
{
    /// <summary>
    /// Polls WASD and forwards the normalized direction to the server:
    /// immediately when it changes (including release) and every resendInterval while in world.
    /// </summary>
    public class MovementInput : MonoBehaviour
    {
        [SerializeField] private NetworkClient client;
        [SerializeField] private float resendInterval = 0.1f;

        private Vector2 lastSent;
        private float nextSendTime;

        private void Update()
        {
            if (client.CurrentState != NetworkClient.State.InWorld) return;

            var direction = ReadDirection();
            if (direction == lastSent && Time.unscaledTime < nextSendTime) return;

            client.SetInput(direction.x, direction.y);
            lastSent = direction;
            nextSendTime = Time.unscaledTime + resendInterval;
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
