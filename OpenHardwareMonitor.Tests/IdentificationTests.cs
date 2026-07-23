using NUnit.Framework;
using OpenHardwareMonitor.Hardware.Mainboard;

namespace OpenHardwareMonitor.Tests
{
    [TestFixture]
    public class IdentificationTests
    {
        [Test]
        public void GetModel_WhenKnownModelPassed_ReturnsCorrectModelEnum()
        {
            // Arrange
            string modelName = "880GMH/USB3";

            // Act
            Model result = Identification.GetModel(modelName);

            // Assert
            Assert.That(result, Is.EqualTo(Model._880GMH_USB3));
        }

        [Test]
        public void GetModel_WhenUnknownModelPassed_ReturnsUnknownEnum()
        {
            // Arrange
            string modelName = "Some Random Unknown Model 1234";

            // Act
            Model result = Identification.GetModel(modelName);

            // Assert
            Assert.That(result, Is.EqualTo(Model.Unknown));
        }

        [Test]
        public void GetModel_WhenAnotherKnownModelPassed_ReturnsCorrectModelEnum()
        {
            // Arrange
            string modelName = "Z390 AORUS ULTRA";

            // Act
            Model result = Identification.GetModel(modelName);

            // Assert
            Assert.That(result, Is.EqualTo(Model.Z390_AORUS_ULTRA));
        }

        [Test]
        public void GetModel_WhenOemModelPassed_ReturnsUnknownEnum()
        {
            // Arrange
            string modelName = "To be filled by O.E.M.";

            // Act
            Model result = Identification.GetModel(modelName);

            // Assert
            Assert.That(result, Is.EqualTo(Model.Unknown));
        }

        [Test]
        public void GetModel_WhenNullPassed_ReturnsUnknownEnum()
        {
            // Arrange
            string modelName = null;

            // Act
            Model result = Identification.GetModel(modelName);

            // Assert
            Assert.That(result, Is.EqualTo(Model.Unknown));
        }
    }
}
