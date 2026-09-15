using System.Collections.Generic;
using UnityEngine;

namespace Demo
{
    /// <summary>
    /// Predicts the local player every frame and reconciles against snapshots: rebuilds the position
    /// from the server's one plus the frames the server hasn't simulated yet, then fades the visual error out.
    /// </summary>
    public class LocalPredictor
    {
        // ponytail: ~7 s of history at 144 fps. Acks trim it to ~40 frames at 200 ms RTT, so the cap only
        // bites when the server stalls; the next reconcile then replays what's left from the server position.
        private const int MaxFrames = 1024;

        private struct Frame
        {
            public long Seq;
            public Vector2 Direction;
            public float Dt;
        }

        private readonly MovementRules rules;
        private readonly float correctionTime;
        private readonly float snapDistance;
        private readonly List<Frame> frames = new List<Frame>();

        private long currentSeq;
        private Vector2 currentDirection;
        private long lastAckSeq = -1;
        // Seconds already removed from the front of the direction run that starts the history.
        private float trimmed;
        private Vector2 trimmedDirection;
        private long trimmedSeq = -1;
        private Vector2 offset;
        private float offsetAtCorrection;

        public LocalPredictor(MovementRules rules, Vector2 spawn, float yaw, float correctionTime, float snapDistance)
        {
            this.rules = rules;
            this.correctionTime = correctionTime;
            this.snapDistance = snapDistance;
            PredictedPosition = spawn;
            Yaw = yaw;
        }

        /// <summary>Predicted position plus the fading visual correction.</summary>
        public Vector2 DisplayPosition => PredictedPosition + offset;
        public Vector2 PredictedPosition { get; private set; }
        /// <summary>Radians, server convention atan2(z, x).</summary>
        public float Yaw { get; private set; }
        public bool IsMoving { get; private set; }
        /// <summary>Meters between the reconciled and the pre-reconcile predicted position.</summary>
        public float LastError { get; private set; }
        public bool LastCorrectionSnapped { get; private set; }

        /// <summary>The input just sent to the server; applies from the next Advance.</summary>
        public void SetInput(long seq, Vector2 direction)
        {
            currentSeq = seq;
            currentDirection = direction;
        }

        public void Advance(float dt)
        {
            if (frames.Count == MaxFrames) TrimFront(frames[0].Dt);
            frames.Add(new Frame { Seq = currentSeq, Direction = currentDirection, Dt = dt });

            PredictedPosition = rules.Step(PredictedPosition, currentDirection, dt);
            IsMoving = currentDirection != Vector2.zero;
            if (IsMoving) Yaw = Mathf.Atan2(currentDirection.y, currentDirection.x);

            offset = correctionTime > 0f
                ? Vector2.MoveTowards(offset, Vector2.zero, offsetAtCorrection * dt / correctionTime)
                : Vector2.zero;
        }

        /// <param name="ackSeq">Newest input the server accepted.</param>
        /// <param name="ageMs">How long the server has integrated the direction ackSeq belongs to.</param>
        /// <param name="serverPosition">Authoritative position after that integration.</param>
        public void Reconcile(long ackSeq, long ageMs, Vector2 serverPosition)
        {
            if (ackSeq < lastAckSeq) return;
            lastAckSeq = ackSeq;

            // The server ages a direction, not a seq: find where ackSeq's same-direction run starts.
            var ack = 0;
            while (ack < frames.Count && frames[ack].Seq < ackSeq) ack++;
            Vector2 direction;
            if (ack < frames.Count && frames[ack].Seq == ackSeq) direction = frames[ack].Direction;
            // ackSeq's frames are all trimmed; trimming never crosses a direction change, so its run is the trimmed one.
            else if (trimmedSeq >= ackSeq) direction = trimmedDirection;
            else
            {
                // No frames of ackSeq (e.g. seq 0 before any input): nothing to skip.
                if (ack > 0) trimmed = 0f;
                frames.RemoveRange(0, ack);
                direction = default;
                ack = -1;
            }

            if (ack >= 0)
            {
                var runStart = ack;
                while (runStart > 0 && frames[runStart - 1].Direction == direction) runStart--;
                if (runStart > 0 || trimmedDirection != direction) trimmed = 0f;
                frames.RemoveRange(0, runStart);
                trimmedDirection = direction;

                // Drop the part of the run the server already credited.
                var skip = ageMs / 1000f - trimmed;
                while (skip > 0f && frames.Count > 0 && frames[0].Direction == direction)
                {
                    var dt = Mathf.Min(skip, frames[0].Dt);
                    TrimFront(dt);
                    skip -= dt;
                }
            }

            var reconciled = serverPosition;
            foreach (var frame in frames) reconciled = rules.Step(reconciled, frame.Direction, frame.Dt);

            var error = reconciled - PredictedPosition;
            LastError = error.magnitude;
            LastCorrectionSnapped = LastError > snapDistance;
            // Keep the displayed position still; Advance fades the offset out over correctionTime.
            offset = LastCorrectionSnapped ? Vector2.zero : offset - error;
            offsetAtCorrection = offset.magnitude;
            PredictedPosition = reconciled;
        }

        /// <summary>Removes dt seconds from the oldest frame, remembering how much of its direction run is gone.</summary>
        private void TrimFront(float dt)
        {
            var front = frames[0];
            if (front.Direction != trimmedDirection) trimmed = 0f;
            trimmedDirection = front.Direction;
            trimmedSeq = front.Seq;
            trimmed += dt;
            front.Dt -= dt;
            if (front.Dt <= 0f) frames.RemoveAt(0);
            else frames[0] = front;
        }
    }
}
