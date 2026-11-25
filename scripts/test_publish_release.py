import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from publish_release import assemble, digest, publish, remaining
from release_contract import marker, required, SCOPES


class PublishTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)

    def stage(self, scope):
        identity = dict(scope=scope, tag="V1.2.3", commit="a"*40, run_id="12",
                        platforms={"android": {"versionCode": 3}})
        for platform in SCOPES[scope]:
            path = self.root / ("release-" + platform)
            path.mkdir(exist_ok=True)
            for name in required(platform, identity["tag"]):
                (path / name).write_text('{"versionCode":3}')
            if platform == "windows":
                name = "MyWubi-components-cpu-1.2.3-windows-x86_64.zip"
                component = dict(os="windows", architecture="x86_64", application="=1.2.3",
                                 source=f"https://github.com/silevilence/MyWubi/releases/download/V1.2.3/{name}",
                                 archive=dict(path=name, bytes=(path / name).stat().st_size, sha256=digest(path / name)))
                (path / "components.json").write_text(json.dumps([component]))
        return identity

    def test_stage_each_scope(self):
        for scope in SCOPES:
            dist = self.root / scope
            assemble(self.root, self.stage(scope), dist)
            expected = required(scope, "V1.2.3") | {marker(scope), "SHA256SUMS.txt"}
            self.assertEqual(set(remaining([], dist)), expected)
            self.assertEqual(len((dist / "SHA256SUMS.txt").read_text().splitlines()), len(expected)-1)

    def test_missing_selected_platform_asset(self):
        identity = self.stage("both")
        (self.root / "release-android/android-update.json").unlink()
        with self.assertRaisesRegex(ValueError, "Missing android"):
            assemble(self.root, identity, self.root / "dist")

    def test_component_cannot_drift_to_latest_or_change_bytes(self):
        identity = self.stage("windows")
        directory = self.root / "release-windows"
        catalog = directory / "components.json"
        original = catalog.read_text()
        catalog.write_text(original.replace("/download/V1.2.3/", "/latest/download/"))
        with self.assertRaisesRegex(ValueError, "not bound"):
            assemble(self.root, identity, self.root / "drift")
        catalog.write_text(original)
        next(directory.glob("MyWubi-components-*.zip")).write_bytes(b"other")
        with self.assertRaisesRegex(ValueError, "corrupt"):
            assemble(self.root, identity, self.root / "corrupt")

    def test_resume_requires_identical_uploaded_bytes(self):
        path = self.root / "one"
        path.write_bytes(b"ok")
        asset = dict(name="one", state="uploaded", digest="sha256:" + digest(path), size=2)
        self.assertEqual(remaining([asset], self.root), [])
        for change in [dict(size=1), dict(digest="wrong"), dict(state="starter"), dict(name="other")]:
            with self.assertRaises(ValueError):
                remaining([asset | change], self.root)

    def test_existing_public_or_mismatched_draft_rejected(self):
        identity = self.stage("android")
        for draft in (False, True):
            release = dict(tag_name=identity["tag"], draft=draft, body="different")
            with patch("publish_release.api", return_value=[[release]]), self.assertRaises(ValueError):
                publish(identity, "notes", self.root)
