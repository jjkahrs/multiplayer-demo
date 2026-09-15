using System.Collections.Generic;
using NUnit.Framework;

namespace Demo.Tests
{
    public class PingSamplerTests
    {
        private static ServerSnapshot Snapshot(params ServerPlayer[] players) =>
            new ServerSnapshot { type = "snapshot", players = new List<ServerPlayer>(players) };

        private static ServerPlayer Player(long id, long seq, long t0) =>
            new ServerPlayer { id = id, name = $"P{id}", seq = seq, t0 = t0 };

        [Test]
        public void PingSampler_BeforeJoin_Null()
        {
            var sampler = new PingSampler();
            Assert.IsFalse(sampler.OnSnapshot(Snapshot(Player(7, 5, 1000)), 1100));
            Assert.IsNull(sampler.PingMs);
        }

        [Test]
        public void PingSampler_SeqZero_NoSample()
        {
            var sampler = new PingSampler();
            sampler.SetLocalPlayer(7);
            Assert.IsFalse(sampler.OnSnapshot(Snapshot(Player(7, 0, 0)), 1789500000000));
            Assert.IsNull(sampler.PingMs);
        }

        [Test]
        public void PingSampler_NewSeq_Samples()
        {
            var sampler = new PingSampler();
            sampler.SetLocalPlayer(7);
            Assert.IsTrue(sampler.OnSnapshot(Snapshot(Player(7, 5, 1000)), 1100));
            Assert.AreEqual(100, sampler.PingMs);
        }

        [Test]
        public void PingSampler_RepeatedSeq_SamplesOnce()
        {
            var sampler = new PingSampler();
            sampler.SetLocalPlayer(7);
            sampler.OnSnapshot(Snapshot(Player(7, 5, 1000)), 1100);

            Assert.IsFalse(sampler.OnSnapshot(Snapshot(Player(7, 5, 1000)), 1300));
            Assert.AreEqual(100, sampler.PingMs);

            Assert.IsTrue(sampler.OnSnapshot(Snapshot(Player(7, 6, 1200)), 1350));
            Assert.AreEqual(150, sampler.PingMs);
        }

        [Test]
        public void PingSampler_NoLocalEntry_KeepsLast()
        {
            var sampler = new PingSampler();
            sampler.SetLocalPlayer(7);
            sampler.OnSnapshot(Snapshot(Player(7, 5, 1000)), 1100);

            Assert.IsFalse(sampler.OnSnapshot(Snapshot(Player(9, 8, 1000)), 1500));
            Assert.AreEqual(100, sampler.PingMs);
        }

        [Test]
        public void PingSampler_OtherPlayersIgnored()
        {
            var sampler = new PingSampler();
            sampler.SetLocalPlayer(7);
            Assert.IsFalse(sampler.OnSnapshot(Snapshot(Player(9, 50, 1000), Player(7, 0, 0)), 1100));
            Assert.IsNull(sampler.PingMs);
        }

        [Test]
        public void PingSampler_NegativeDelta_ClampsToZero()
        {
            var sampler = new PingSampler();
            sampler.SetLocalPlayer(7);
            sampler.OnSnapshot(Snapshot(Player(7, 1, 2000)), 1900);
            Assert.AreEqual(0, sampler.PingMs);
        }

        [Test]
        public void PingSampler_SetLocalPlayer_ClearsPing()
        {
            var sampler = new PingSampler();
            sampler.SetLocalPlayer(7);
            sampler.OnSnapshot(Snapshot(Player(7, 5, 1000)), 1100);

            sampler.SetLocalPlayer(8);
            Assert.IsNull(sampler.PingMs);

            Assert.IsTrue(sampler.OnSnapshot(Snapshot(Player(8, 1, 2000)), 2040));
            Assert.AreEqual(40, sampler.PingMs);
        }
    }
}
