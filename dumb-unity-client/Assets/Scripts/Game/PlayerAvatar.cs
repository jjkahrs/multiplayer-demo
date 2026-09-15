using TMPro;
using UnityEngine;

namespace Demo
{
    public class PlayerAvatar : MonoBehaviour
    {
        private static readonly int MovingHash = Animator.StringToHash("Moving");

        [SerializeField] private Animator animator;
        [SerializeField] private TMP_Text label;
        [SerializeField] private Color localColor = new Color(1f, 0.85f, 0.2f);
        [SerializeField] private Color remoteColor = Color.white;
        [SerializeField] private float labelReferenceDistance = 50f;
        [SerializeField] private float minLabelScale = 0.8f;
        [SerializeField] private float maxLabelScale = 1.3f;
        [SerializeField] private float localLabelScale = 1.3f;

        private float labelScale = 1f;

        public void SetState(string state) => animator.SetBool(MovingHash, state == "walk");

        public void SetName(string displayName) => label.text = displayName;

        public void SetLocal(bool isLocal)
        {
            label.color = isLocal ? localColor : remoteColor;
            label.fontStyle = isLocal ? FontStyles.Bold : FontStyles.Normal;
            label.GetComponent<Renderer>().sortingOrder = isLocal ? 1 : 0;
            labelScale = isLocal ? localLabelScale : 1f;
        }

        private void LateUpdate()
        {
            var cam = Camera.main;
            if (cam == null) return;
            var camTransform = cam.transform;
            label.transform.rotation = camTransform.rotation;
            // Near-constant on-screen size; clamped so far rows, which bunch up on screen, don't overlap into clumps.
            var distance = Vector3.Distance(camTransform.position, label.transform.position);
            var distanceScale = Mathf.Clamp(distance / labelReferenceDistance, minLabelScale, maxLabelScale);
            label.transform.localScale = Vector3.one * (labelScale * distanceScale);
        }
    }
}
