"""Portable packaging checks; no Apple credentials, SDK, or device required."""

import importlib.util
from pathlib import Path
import plistlib
import stat
import struct
import tempfile
import unittest
import zipfile


spec = importlib.util.spec_from_file_location("unsigned_ipa", Path(__file__).resolve().parents[1] / "scripts/package-unsigned-ipa.py")
ipa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ipa)


def executable(platform=2, cpu=0x100000C, signed=False):
    commands = struct.pack("<6I", 0x32, 24, platform, 0x110000, 0x1A0000, 0)
    if signed:
        commands += struct.pack("<4I", 0x1D, 16, 0, 4)
    return struct.pack("<8I", 0xFEEDFACF, cpu, 0, 2, 1 + signed, len(commands), 0, 0) + commands + b"body"


class UnsignedIPATests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root / "Reader.xcarchive"
        self.app = self.archive / "Products/Applications/ReaderPreview.app"
        self.app.mkdir(parents=True)
        self.info = {"CFBundlePackageType": "APPL", "CFBundleExecutable": "ReaderPreview",
                     "CFBundleIdentifier": "local.reader.ipa-check", "CFBundleSupportedPlatforms": ["iPhoneOS"],
                     "CFBundleShortVersionString": "1.0.8", "DTPlatformName": "iphoneos"}
        self.write_info()
        self.binary = self.app / "ReaderPreview"
        self.binary.write_bytes(executable())
        self.binary.chmod(0o755)
        self.output = self.root / "out"

    def write_info(self):
        (self.app / "Info.plist").write_bytes(plistlib.dumps(self.info))

    def pack(self):
        result = ipa.package(self.archive, self.output, "v1.0.8")
        return self.output / result["file"]

    def test_payload_preserves_frameworks_unicode_files_and_executable_bits(self):
        resource = self.app / "Resources/中文样张.txt"
        resource.parent.mkdir()
        resource.write_text("font sample")
        framework = self.app / "Frameworks/Engine.framework/Engine"
        framework.parent.mkdir(parents=True)
        framework.write_bytes(b"embedded framework")
        framework.chmod(0o755)
        (self.app / "Resources/Current.txt").symlink_to("中文样张.txt")
        (self.archive / "dSYMs").mkdir()
        (self.archive / "dSYMs/debug").write_bytes(b"not part of IPA")
        before = self.binary.read_bytes()
        output = self.pack()
        self.assertEqual(ipa.verify(output, str(output) + ".sha256")["platform"], "iPhoneOS")
        self.assertEqual(before, self.binary.read_bytes())
        with zipfile.ZipFile(output) as bundle:
            self.assertEqual(bundle.read("Payload/ReaderPreview.app/Frameworks/Engine.framework/Engine"), framework.read_bytes())
            self.assertEqual(bundle.read("Payload/ReaderPreview.app/Resources/中文样张.txt"), resource.read_bytes())
            self.assertTrue(stat.S_ISLNK(bundle.getinfo("Payload/ReaderPreview.app/Resources/Current.txt").external_attr >> 16))
            self.assertFalse(any("dSYMs" in name for name in bundle.namelist()))

    def test_arm64_simulator_binary_is_rejected_even_with_device_plist(self):
        self.binary.write_bytes(executable(platform=7))
        with self.assertRaisesRegex(ValueError, "physical iOS"):
            self.pack()

    def test_simulator_plist_and_wrong_cpu_are_rejected(self):
        self.info["CFBundleSupportedPlatforms"] = ["iPhoneSimulator"]
        self.write_info()
        with self.assertRaisesRegex(ValueError, "iPhoneOS"):
            self.pack()
        self.info["CFBundleSupportedPlatforms"] = ["iPhoneOS"]
        self.write_info()
        self.binary.write_bytes(executable(cpu=0x1000007))
        with self.assertRaisesRegex(ValueError, "arm64"):
            self.pack()

    def test_signed_executable_and_provisioning_profile_are_rejected(self):
        self.binary.write_bytes(executable(signed=True))
        with self.assertRaisesRegex(ValueError, "already signed"):
            self.pack()
        self.binary.write_bytes(executable())
        (self.app / "embedded.mobileprovision").write_bytes(b"profile")
        with self.assertRaisesRegex(ValueError, "Signing metadata"):
            self.pack()

    def test_invalid_metadata_and_missing_execute_permission_are_rejected(self):
        self.info["CFBundleExecutable"] = "../outside"
        self.write_info()
        with self.assertRaisesRegex(ValueError, "executable name"):
            self.pack()
        self.info["CFBundleExecutable"] = "ReaderPreview"
        self.write_info()
        self.binary.chmod(0o644)
        with self.assertRaisesRegex(ValueError, "permissions"):
            self.pack()

    def test_bad_build_preserves_existing_package(self):
        output = self.pack()
        before = output.read_bytes()
        self.binary.write_bytes(b"corrupt")
        with self.assertRaises(ValueError):
            self.pack()
        self.assertEqual(before, output.read_bytes())
        self.assertEqual([], list(self.output.glob(".unsigned-ipa-*")))

    def test_checksum_mismatch_and_unexpected_zip_content_are_rejected(self):
        output = self.pack()
        checksum = Path(str(output) + ".sha256")
        checksum.write_text("0" * 64 + "  " + output.name)
        with self.assertRaisesRegex(ValueError, "checksum"):
            ipa.verify(output, checksum)
        with zipfile.ZipFile(output, "a") as bundle:
            bundle.writestr("outside.txt", b"unexpected")
        with self.assertRaisesRegex(ValueError, "outside Payload"):
            ipa.verify(output)

    def test_unsafe_version_and_symlink_are_rejected(self):
        with self.assertRaisesRegex(ValueError, "safe filename"):
            ipa.package(self.archive, self.output, "../other")
        (self.root / "private").write_text("outside")
        (self.app / "outside").symlink_to(self.root / "private")
        with self.assertRaisesRegex(ValueError, "symlink escapes"):
            self.pack()


if __name__ == "__main__":
    unittest.main()
