using System;
using NUnit.Framework;

namespace OpenHardwareMonitor.Hardware.Mainboard.Tests
{
    [TestFixture]
    public class IdentificationTest
    {
        [Test]
        public void GetManufacturer_KnownName_ReturnsCorrectManufacturer()
        {
            Assert.AreEqual(Manufacturer.Abit, Identification.GetManufacturer("ABIT"));
            Assert.AreEqual(Manufacturer.Abit, Identification.GetManufacturer("abit"));
            Assert.AreEqual(Manufacturer.Acer, Identification.GetManufacturer("Acer"));
            Assert.AreEqual(Manufacturer.ASUS, Identification.GetManufacturer("ASUSTek Computer Inc."));
            Assert.AreEqual(Manufacturer.ASUS, Identification.GetManufacturer("ASUSTeK COMPUTER INC."));
            Assert.AreEqual(Manufacturer.Gigabyte, Identification.GetManufacturer("Gigabyte"));
            Assert.AreEqual(Manufacturer.Gigabyte, Identification.GetManufacturer("Gigabyte Technology Co., Ltd."));
            Assert.AreEqual(Manufacturer.MSI, Identification.GetManufacturer("Micro-Star International Co., Ltd."));
            Assert.AreEqual(Manufacturer.Intel, Identification.GetManufacturer("Intel Corporation"));
            Assert.AreEqual(Manufacturer.Abit, Identification.GetManufacturer("http://www.abit.com.tw/"));
            Assert.AreEqual(Manufacturer.Abit, Identification.GetManufacturer("www.abit.com.tw"));
        }

        [Test]
        public void GetManufacturer_OEMName_ReturnsUnknown()
        {
            Assert.AreEqual(Manufacturer.Unknown, Identification.GetManufacturer("To be filled by O.E.M."));
        }

        [Test]
        public void GetManufacturer_UnknownName_ReturnsUnknown()
        {
            Assert.AreEqual(Manufacturer.Unknown, Identification.GetManufacturer("SomeUnknownManufacturerXYZ"));
            Assert.AreEqual(Manufacturer.Unknown, Identification.GetManufacturer(""));
            Assert.AreEqual(Manufacturer.Unknown, Identification.GetManufacturer(null));
        }
    }
}
