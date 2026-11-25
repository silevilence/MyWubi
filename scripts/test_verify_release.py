import unittest
from unittest.mock import patch

from release_contract import SCOPES, marker, required
from verify_release import verify


class VerifyTests(unittest.TestCase):
    def test_each_scope_reads_only_metadata_and_its_scope_record(self):
        for scope in SCOPES:
            tag = "V0.14.0"
            sha = "a" * 40
            names = sorted(required(scope, tag) | {"SHA256SUMS.txt", marker(scope)})
            assets = [dict(name=n, state="uploaded", digest="sha256:" + "d"*64, size=50, id=i)
                      for i, n in enumerate(names)]
            release = dict(tag_name=tag, draft=False, prerelease=False, published_at="today",
                           body="notes", html_url="release-url", assets=assets)
            record = dict(tag=tag, scope=scope, commit=sha, run_id="42",
                          platforms={p: dict(tag=tag, commit=sha) for p in SCOPES[scope]},
                          assets={a["name"]: {"size": a["size"], "digest": a["digest"]} for a in assets
                                  if a["name"] not in {marker(scope), "SHA256SUMS.txt"}})
            endpoints = {"releases?per_page=100": [[]], "releases/tags/" + tag: release, "commits/" + tag: {"sha": sha},
                         f"releases/assets/{names.index(marker(scope))}": record}
            with patch("verify_release.api", side_effect=lambda path, *args: endpoints[path]):
                self.assertEqual(verify(tag, scope, sha, "42", "notes")[0], "release-url")
                original = record["platforms"]
                record["platforms"] = {}
                with self.assertRaisesRegex(ValueError, "platform history"):
                    verify(tag, scope, sha, "42", "notes")
                record["platforms"] = original
                extra = assets[0] | dict(name="MyWubi-0.14.0-delta.nupkg")
                if scope == "android":
                    release["assets"] = [*assets, extra]
                    with self.assertRaises(ValueError):
                        verify(tag, scope, sha, "42", "notes")
                release["assets"] = [a for a in assets if a["name"] != "SHA256SUMS.txt"]
                with self.assertRaisesRegex(ValueError, "Missing"):
                    verify(tag, scope, sha, "42", "notes")
