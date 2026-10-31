import tempfile
from pathlib import Path
import unittest

from release_metadata import prepare


class ReleaseMetadataTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        (self.root / "Cargo.toml").write_text('[workspace]\nmembers=["core"]\n[workspace.package]\nversion="0.5.0"\n')
        (self.root / "core").mkdir()
        (self.root / "core/Cargo.toml").write_text('[package]\nversion.workspace=true\n')
        (self.root / "android/app").mkdir(parents=True)
        (self.root / "android/app/build.gradle").write_text("versionName '0.5.0'\nversionCode providers.gradleProperty('versionCode').getOrElse('3').toInteger()\n")
        (self.root / "CHANGELOG.md").write_text('# Changes\n\n## V0.5.0\n\nNew APK\n\n## V0.4.0\n\nOld release\n')

    def test_extracts_exact_section_and_supports_tag_case(self):
        for tag in ("V0.5.0", "v0.5.0"):
            result = prepare(self.root, f"refs/tags/{tag}", self.root / "output")
            self.assertEqual(result, {"version": "0.5.0", "tag": tag, "version_code": "3"})
            self.assertEqual((self.root / "output/release-notes.md").read_text(), "New APK\n")

    def test_rejects_wrong_tag_and_android_version(self):
        with self.assertRaises(ValueError):
            prepare(self.root, "refs/tags/V0.4.0", self.root / "output")
        file = self.root / "android/app/build.gradle"
        file.write_text(file.read_text().replace("0.5.0", "0.4.0"))
        with self.assertRaises(ValueError):
            prepare(self.root, "refs/tags/V0.5.0", self.root / "output")

    def test_rejects_missing_or_duplicate_notes_and_member_mismatch(self):
        file = self.root / "CHANGELOG.md"
        for content in ("# No notes", "## V0.5.0\nFirst\n## V0.5.0\nDuplicate"):
            file.write_text(content)
            with self.assertRaises(ValueError):
                prepare(self.root, "", self.root / "output")
        (self.root / "core/Cargo.toml").write_text('[package]\nversion="0.4.0"\n')
        with self.assertRaises(ValueError):
            prepare(self.root, "", self.root / "output")


if __name__ == "__main__":
    unittest.main()
