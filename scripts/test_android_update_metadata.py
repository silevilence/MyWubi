import tempfile
import unittest
from pathlib import Path
from android_update_metadata import metadata


class AndroidUpdateMetadataTests(unittest.TestCase):
    def test_manifest_identity_hash_and_compatibility(self):
        with tempfile.TemporaryDirectory() as directory:
            apk = Path(directory) / "app.apk"
            apk.write_bytes(b"signed fixture")
            badging = "package: name='org.mywubi' versionCode='5' versionName='0.5.1'\nsdkVersion:'26'\nnative-code: 'arm64-v8a' 'x86_64'"
            result = metadata(badging, apk, "v0.5.1", "修复")
            self.assertEqual(result["versionCode"], 5)
            self.assertEqual(result["minSdk"], 26)
            self.assertEqual(len(result["sha256"]), 64)
            self.assertEqual(result["notes"], "修复")
            for invalid in (badging.replace("org.mywubi", "evil.app"), badging.replace("0.5.1", "0.5.2"),
                            badging.replace("versionCode='5'", "versionCode='2100000001'"),
                            badging.replace("sdkVersion:'26'", "sdkVersion:'25'"),
                            badging.replace("arm64-v8a", "armeabi-v7a"), "invalid"):
                with self.assertRaises(ValueError):
                    metadata(invalid, apk, "v0.5.1", "")
