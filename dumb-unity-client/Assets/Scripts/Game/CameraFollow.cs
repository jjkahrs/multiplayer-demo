using UnityEngine;

namespace Demo
{
    public class CameraFollow : MonoBehaviour
    {
        public Transform target;

        [SerializeField] private Vector3 offset = new Vector3(0f, 40f, -30f);
        [SerializeField] private float damping = 5f;

        private void LateUpdate()
        {
            if (target == null) return;

            var desired = target.position + offset;
            transform.position = Vector3.Lerp(transform.position, desired, damping * Time.deltaTime);
            transform.LookAt(target);
        }
    }
}
