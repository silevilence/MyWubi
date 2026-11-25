import itertools
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from publish_release import assemble, check_jobs, digest, publish, remaining
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
            if platform == "android":
                for suffix in ("", "-full"):
                    name = f"MyWubi-1.2.3-Android{suffix}.apk"
                    sha = digest(path / name)
                    (path / f"MyWubi-1.2.3-Android{suffix}-native.json").write_text(json.dumps(dict(sha256=sha)))
                    (path / f"android-update{suffix}.json").write_text(json.dumps(dict(
                        sha256=sha, versionName="1.2.3", versionCode=3, notes="新版输入法 ✨",
                        apkUrl=f"https://github.com/silevilence/MyWubi/releases/download/V1.2.3/{name}"), ensure_ascii=False), encoding="utf-8")
            if platform == "windows":
                name = "MyWubi-components-cpu-1.2.3-windows-x86_64.zip"
                component = dict(os="windows", architecture="x86_64", application="=1.2.3",
                                 source=f"https://github.com/silevilence/MyWubi/releases/download/V1.2.3/{name}",
                                 archive=dict(path=name, bytes=(path / name).stat().st_size, sha256=digest(path / name)))
                (path / "components.json").write_text(json.dumps([component]))
        return identity

    def test_workflow_only_publishes_the_exact_selected_results(self):
        states = ("success", "skipped", "failure", "cancelled")
        for scope, selected in SCOPES.items():
            for win, android in itertools.product(states, repeat=2):
                results = {"windows": win, "android": android}
                if all(results[p] == ("success" if p in selected else "skipped") for p in results):
                    check_jobs(scope, results)
                else:
                    with self.assertRaises(ValueError):
                        check_jobs(scope, results)

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

    def test_resume_original_run_after_partial_upload(self):
        identity = self.stage("android")
        dist = self.root / "dist"
        assemble(self.root, identity, dist)
        draft = {}
        uploaded = []

        def fake_api(path, *args):
            if path.startswith("releases?"):
                return [[draft]] if draft else [[]]
            if path == "releases":
                field = lambda name: next(a.split("=", 1)[1] for a in args if a.startswith(name + "="))
                draft.update(id=1, tag_name=field("tag_name"), body=field("body"),
                             target_commitish=field("target_commitish"), draft=True)
            elif path.startswith("releases/1/assets"):
                return [uploaded]
            elif path == "releases/1":
                draft["draft"] = False
            else:
                self.fail(path)
            return draft

        def upload(*args):
            path = args[-1]
            uploaded.append(dict(name=path.name, size=path.stat().st_size, state="uploaded",
                                 digest="sha256:" + digest(path)))

        with patch("publish_release.api", side_effect=fake_api):
            def interrupted(*args):
                if uploaded:
                    raise OSError("offline")
                upload(*args)
            with patch("publish_release.gh", side_effect=interrupted), self.assertRaises(OSError):
                publish(identity, "notes", dist)
            self.assertTrue(draft["draft"])
            with self.assertRaises(ValueError):
                publish(identity | {"run_id": "different"}, "notes", dist)
            with patch("publish_release.gh", side_effect=upload):
                publish(identity, "notes", dist)
            self.assertFalse(draft["draft"])
            self.assertEqual(len(uploaded), len(list(dist.iterdir())))

    def test_existing_public_or_mismatched_draft_rejected(self):
        identity = self.stage("android")
        for draft in (False, True):
            release = dict(tag_name=identity["tag"], draft=draft, body="different")
            with patch("publish_release.api", return_value=[[release]]), self.assertRaises(ValueError):
                publish(identity, "notes", self.root)
