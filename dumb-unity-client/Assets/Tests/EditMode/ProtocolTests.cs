using NUnit.Framework;

namespace Demo.Tests
{
    // JSON literals are the server's exact output (see dumb-server/crates/protocol/tests/serde.rs).
    public class ProtocolTests
    {
        [Test]
        public void JoinSerializesToWireShape()
        {
            Assert.AreEqual("{\"type\":\"join\",\"name\":\"Bob\"}", Protocol.ToJson(new ClientJoin("Bob")));
        }

        [Test]
        public void InputSerializesWithTypeTagAndFields()
        {
            var json = Protocol.ToJson(new ClientInput { vx = 0, vz = 1, seq = 42, t0 = 1757900000000 });
            StringAssert.StartsWith("{\"type\":\"input\",", json);
            var back = Protocol.FromJson<ClientInput>(json);
            Assert.AreEqual(0, back.vx, 1e-9);
            Assert.AreEqual(1, back.vz, 1e-9);
            Assert.AreEqual(42, back.seq);
            Assert.AreEqual(1757900000000, back.t0);
        }

        [Test]
        public void ParsesJoined()
        {
            var msg = Protocol.ToMessage(
                "{\"type\":\"joined\",\"playerId\":7,\"name\":\"Bob\",\"x\":0.0,\"z\":0.0,\"yaw\":1.5708,\"speed\":5.0,\"worldHalf\":50.0,\"tickHz\":20}");
            var joined = msg as ServerJoined;
            Assert.IsNotNull(joined);
            Assert.AreEqual(7, joined.playerId);
            Assert.AreEqual("Bob", joined.name);
            Assert.AreEqual(1.5708, joined.yaw, 1e-9);
            Assert.AreEqual(5.0, joined.speed, 1e-9);
            Assert.AreEqual(50.0, joined.worldHalf, 1e-9);
            Assert.AreEqual(20, joined.tickHz);
        }

        [Test]
        public void ParsesSnapshot()
        {
            var msg = Protocol.ToMessage(
                "{\"type\":\"snapshot\",\"tick\":1234,\"players\":[{\"id\":7,\"name\":\"Bob\",\"x\":12.0,\"z\":-3.5,\"yaw\":2.0,\"state\":\"walk\",\"seq\":42,\"t0\":912345,\"ageMs\":150}]}");
            var snapshot = msg as ServerSnapshot;
            Assert.IsNotNull(snapshot);
            Assert.AreEqual(1234, snapshot.tick);
            Assert.AreEqual(1, snapshot.players.Count);
            var p = snapshot.players[0];
            Assert.AreEqual(7, p.id);
            Assert.AreEqual(12.0, p.x, 1e-9);
            Assert.AreEqual(-3.5, p.z, 1e-9);
            Assert.AreEqual("walk", p.state);
            Assert.AreEqual(42, p.seq);
            Assert.AreEqual(912345, p.t0);
            Assert.AreEqual(150, p.ageMs);
        }

        [Test]
        public void ParsesPlayerJoinedLeftAndError()
        {
            Assert.AreEqual(9, (Protocol.ToMessage("{\"type\":\"playerJoined\",\"id\":9,\"name\":\"Alice\"}") as ServerPlayerJoined)?.id);
            Assert.AreEqual(9, (Protocol.ToMessage("{\"type\":\"playerLeft\",\"id\":9}") as ServerPlayerLeft)?.id);
            var error = Protocol.ToMessage("{\"type\":\"error\",\"code\":\"bad_name\",\"message\":\"name too long\"}") as ServerErrorMsg;
            Assert.AreEqual("bad_name", error?.code);
            Assert.AreEqual("name too long", error?.message);
        }

        [Test]
        public void UnknownOrMalformedFramesReturnNull()
        {
            Assert.IsNull(Protocol.ToMessage("{\"type\":\"nope\"}"));
            Assert.IsNull(Protocol.ToMessage("not json"));
        }
    }
}
