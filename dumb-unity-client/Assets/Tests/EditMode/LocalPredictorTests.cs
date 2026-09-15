using System;
using System.Collections.Generic;
using NUnit.Framework;
using UnityEngine;

namespace Demo.Tests
{
    public class LocalPredictorTests
    {
        private const float FrameDt = 1f / 144f;
        private static readonly MovementRules Rules = new MovementRules(5f, 50f, 20);

        private static LocalPredictor NewPredictor() => new LocalPredictor(Rules, Vector2.zero, 0f, 0.1f, 1f);

        /// <summary>
        /// Client at 144 Hz sending like MovementInput (on change plus every 100 ms), and a server model that
        /// applies inputs at arrival, integrates on 50 ms tick boundaries crediting the whole tick to the
        /// current direction, and delivers snapshots after a fixed one-way delay.
        /// </summary>
        private class Sim
        {
            private const double TickDt = 0.05;
            private const double Delay = 0.1;
            private const double ResendInterval = 0.1;

            private struct Input { public double At; public long Seq; public Vector2 Direction; }
            private struct Snapshot { public double At; public long Seq; public long AgeMs; public Vector2 Position; }

            public readonly LocalPredictor Predictor = NewPredictor();
            public Action<long> OnReconciled;
            public Vector2 ServerPosition { get; private set; }
            public double Time => frame / 144.0;
            public long Seq { get; private set; }

            private readonly Queue<Input> inbox = new Queue<Input>();
            private readonly Queue<Snapshot> outbox = new Queue<Snapshot>();
            private long frame;
            private Vector2 sentDirection;
            private double nextSend;
            private Vector2 serverDirection;
            private long serverSeq;
            private double serverAge;
            private double nextTick = TickDt;

            public void Frame(Vector2 direction)
            {
                while (nextTick <= Time) ServerTick();
                while (outbox.Count > 0 && outbox.Peek().At <= Time)
                {
                    var s = outbox.Dequeue();
                    Predictor.Reconcile(s.Seq, s.AgeMs, s.Position);
                    OnReconciled?.Invoke(s.Seq);
                }
                if (Seq == 0 || direction != sentDirection || Time >= nextSend)
                {
                    Seq++;
                    sentDirection = direction;
                    nextSend = Time + ResendInterval;
                    Predictor.SetInput(Seq, direction);
                    inbox.Enqueue(new Input { At = Time + Delay, Seq = Seq, Direction = direction });
                }
                Predictor.Advance(FrameDt);
                frame++;
            }

            private void ServerTick()
            {
                while (inbox.Count > 0 && inbox.Peek().At <= nextTick)
                {
                    var input = inbox.Dequeue();
                    if (input.Direction != serverDirection) serverAge = 0; // player.rs: only a new direction resets the age
                    serverDirection = input.Direction;
                    serverSeq = input.Seq;
                }
                serverAge += TickDt;
                ServerPosition = Rules.Step(ServerPosition, serverDirection, (float)TickDt);
                outbox.Enqueue(new Snapshot
                {
                    At = nextTick + Delay,
                    Seq = serverSeq,
                    AgeMs = (long)Math.Round(serverAge * 1000),
                    Position = ServerPosition,
                });
                nextTick += TickDt;
            }
        }

        [Test]
        public void MovementRules_ClampsToWorldHalf()
        {
            var position = Rules.Step(new Vector2(49f, -49f), Vector2.right, 1f);
            Assert.AreEqual(50f, position.x);
            position = Rules.Step(position, Vector2.down, 1f);
            Assert.AreEqual(-50f, position.y);
            position = Rules.Step(position, Vector2.left, 1000f);
            Assert.AreEqual(-50f, position.x);
        }

        [Test]
        public void SetInputThenAdvance_MovesAndFaces()
        {
            var predictor = NewPredictor();
            predictor.SetInput(1, Vector2.up);
            predictor.Advance(FrameDt);

            Assert.Greater(predictor.PredictedPosition.y, 0f);
            Assert.AreEqual(Mathf.PI / 2f, predictor.Yaw, 1e-5f);
            Assert.IsTrue(predictor.IsMoving);
        }

        [Test]
        public void Reconcile_SteadyWalk_ErrorNearZero()
        {
            var sim = new Sim();
            var reconciles = 0;
            var maxError = 0f;
            sim.OnReconciled = _ =>
            {
                reconciles++;
                maxError = Mathf.Max(maxError, sim.Predictor.LastError);
                Assert.IsFalse(sim.Predictor.LastCorrectionSnapped, $"snap at t={sim.Time:F3}");
            };

            for (var i = 0; i < 3 * 144; i++) sim.Frame(Vector2.up);

            Assert.Greater(reconciles, 50);
            Assert.Less(maxError, 0.01f);
        }

        [Test]
        public void Reconcile_AfterStop_ConvergesWithinTolerance()
        {
            var predictor = NewPredictor();
            predictor.SetInput(1, Vector2.up);
            for (var i = 0; i < 10; i++) predictor.Advance(FrameDt);
            predictor.SetInput(2, Vector2.zero);
            predictor.Advance(FrameDt);
            var stopped = predictor.PredictedPosition;
            predictor.Advance(FrameDt);
            Assert.AreEqual(stopped, predictor.PredictedPosition, "stops within one Advance");
            Assert.IsFalse(predictor.IsMoving);

            var sim = new Sim();
            for (var i = 0; i < 144; i++) sim.Frame(Vector2.up);
            sim.Frame(Vector2.zero);
            var stopSeq = sim.Seq;
            var ackTime = -1.0;
            sim.OnReconciled = seq =>
            {
                if (ackTime < 0 && seq >= stopSeq) ackTime = sim.Time;
            };
            while ((ackTime < 0 || sim.Time < ackTime + 0.25) && sim.Time < 5.0) sim.Frame(Vector2.zero);

            Assert.GreaterOrEqual(ackTime, 0.0, "stop was acked");
            Assert.LessOrEqual(Vector2.Distance(sim.Predictor.DisplayPosition, sim.ServerPosition), 0.3f);
        }

        [Test]
        public void Reconcile_LargeError_Snaps()
        {
            var predictor = NewPredictor();
            predictor.SetInput(1, Vector2.up);
            predictor.Advance(FrameDt);

            predictor.Reconcile(1, 0, new Vector2(5f, 0f));

            Assert.IsTrue(predictor.LastCorrectionSnapped);
            Assert.AreEqual(predictor.PredictedPosition, predictor.DisplayPosition);
        }

        [Test]
        public void Reconcile_StaleAck_Ignored()
        {
            var predictor = NewPredictor();
            predictor.SetInput(1, Vector2.up);
            for (var i = 0; i < 5; i++) predictor.Advance(FrameDt);
            predictor.SetInput(2, Vector2.up);
            for (var i = 0; i < 5; i++) predictor.Advance(FrameDt);
            predictor.Reconcile(2, 0, Vector2.zero);
            var predicted = predictor.PredictedPosition;
            var display = predictor.DisplayPosition;
            var error = predictor.LastError;

            predictor.Reconcile(1, 0, new Vector2(3f, 3f));

            Assert.AreEqual(predicted, predictor.PredictedPosition);
            Assert.AreEqual(display, predictor.DisplayPosition);
            Assert.AreEqual(error, predictor.LastError);
        }
    }
}
