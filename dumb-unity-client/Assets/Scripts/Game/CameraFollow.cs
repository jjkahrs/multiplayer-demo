using UnityEngine;
using UnityEngine.InputSystem;

namespace Demo
{
    public class CameraFollow : MonoBehaviour
    {
        public Transform target;

        [SerializeField] private Vector3 offset = new Vector3(0f, 40f, -30f);
        [SerializeField] private float damping = 5f;
        [SerializeField] private float minZoom = 0.3f;
        [SerializeField] private float maxZoom = 2f;
        [SerializeField] private float zoomStep = 0.1f;

        // Multiplier on offset length; direction (view angle) never changes.
        public float Zoom { get; set; } = 1f;

        private void Update()
        {
            var mouse = Mouse.current;
            if (mouse == null) return;

            // ponytail: one step per frame with scroll input, ignores wheel magnitude (differs per platform/device)
            var scroll = mouse.scroll.ReadValue().y;
            if (scroll > 0f) Zoom *= 1f - zoomStep;
            else if (scroll < 0f) Zoom /= 1f - zoomStep;
            Zoom = Mathf.Clamp(Zoom, minZoom, maxZoom);
        }

        private void LateUpdate()
        {
            if (target == null) return;

            var desired = target.position + offset * Zoom;
            transform.position = Vector3.Lerp(transform.position, desired, damping * Time.deltaTime);
            transform.LookAt(target);
        }
    }
}
