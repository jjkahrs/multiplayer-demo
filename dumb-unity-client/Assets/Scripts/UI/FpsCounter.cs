using UnityEngine;

namespace Demo
{
    /// <summary>
    /// Average frame rate over fixed windows of unscaled time. Fps updates once per completed window.
    /// </summary>
    public class FpsCounter
    {
        private readonly float windowSeconds;
        private int frames;
        private float elapsed;

        public FpsCounter(float windowSeconds = 0.5f) => this.windowSeconds = windowSeconds;

        /// <summary>0 until the first window completes.</summary>
        public int Fps { get; private set; }

        /// <summary>Adds one frame. Returns true when a window completed and Fps changed value.</summary>
        public bool Tick(float unscaledDeltaTime)
        {
            frames++;
            elapsed += unscaledDeltaTime;
            if (elapsed < windowSeconds) return false;

            // frames / elapsed is the true rate; averaging 1/dt would let one long frame vanish in the mean.
            var next = Mathf.RoundToInt(frames / elapsed);
            frames = 0;
            elapsed = 0f;
            var changed = next != Fps;
            Fps = next;
            return changed;
        }

        public void Reset()
        {
            frames = 0;
            elapsed = 0f;
            Fps = 0;
        }
    }
}
