using NUnit.Framework;

namespace Demo.Tests
{
    public class JoinScreenArgsTests
    {
        [Test]
        public void ReturnsValueAfterKey()
        {
            var args = new[] { "dumb-client.exe", "-screen-width", "1280", "-name", "BuildB" };
            Assert.AreEqual("BuildB", JoinScreen.ArgValue(args, "-name"));
        }

        [Test]
        public void ReturnsNullWhenKeyAbsent()
        {
            Assert.IsNull(JoinScreen.ArgValue(new[] { "dumb-client.exe", "-name", "BuildB" }, "-host"));
        }

        [Test]
        public void ReturnsNullWhenKeyIsLastArg()
        {
            Assert.IsNull(JoinScreen.ArgValue(new[] { "dumb-client.exe", "-name" }, "-name"));
        }
    }
}
