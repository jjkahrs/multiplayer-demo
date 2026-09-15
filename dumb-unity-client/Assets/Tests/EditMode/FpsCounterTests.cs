using NUnit.Framework;

namespace Demo.Tests
{
    public class FpsCounterTests
    {
        [Test]
        public void FpsCounter_ConstantDt_ReportsRate()
        {
            var counter = new FpsCounter();
            for (int i = 0; i < 144; i++) counter.Tick(1f / 144f);
            Assert.AreEqual(144, counter.Fps);
        }

        [Test]
        public void FpsCounter_ReportsOnlyAfterWindow()
        {
            var counter = new FpsCounter();
            for (int i = 0; i < 4; i++) Assert.IsFalse(counter.Tick(0.1f), $"tick {i}");
            Assert.AreEqual(0, counter.Fps);

            Assert.IsTrue(counter.Tick(0.2f));
            Assert.AreEqual(8, counter.Fps); // 5 frames / 0.6 s
        }

        [Test]
        public void FpsCounter_MixedFrames_UsesFramesOverElapsed()
        {
            var counter = new FpsCounter();
            for (int i = 0; i < 9; i++) counter.Tick(0.01f);
            Assert.IsTrue(counter.Tick(0.42f));
            // 10 frames / 0.51 s ≈ 19.6 → 20. A mean of 1/dt would read ≈ 90.
            Assert.AreEqual(20, counter.Fps);
        }

        [Test]
        public void FpsCounter_SameRate_ReturnsFalse()
        {
            var counter = new FpsCounter();
            for (int i = 0; i < 3; i++) counter.Tick(0.125f);
            Assert.IsTrue(counter.Tick(0.125f));
            Assert.AreEqual(8, counter.Fps);

            for (int i = 0; i < 3; i++) counter.Tick(0.125f);
            Assert.IsFalse(counter.Tick(0.125f));
            Assert.AreEqual(8, counter.Fps);
        }
    }
}
