"""Verify a public release from GitHub metadata and its small scope record."""
import argparse
import re
from pathlib import Path

from publish_release import api
from release_contract import SCOPES, TAG, baselines, marker, platforms, required, version


def verify(tag, scope, commit, run, notes):
    release = api("releases/tags/" + tag)
    if (release["tag_name"] != tag or release["draft"] or release["prerelease"]
            or not release["published_at"] or api("commits/" + tag)["sha"] != commit):
        raise ValueError("Release status or tag commit differs")
    if release["body"].split("<!-- mywubi-release", 1)[0].strip() != notes.strip():
        raise ValueError("Published release notes differ")
    assets = {a["name"]: a for a in release["assets"]}
    names = set(assets)
    expected = required(scope, tag) | {marker(scope), "SHA256SUMS.txt"}
    if expected - names:
        raise ValueError(f"Missing release assets: {sorted(expected - names)}")
    if platforms(names) != platforms([marker(scope)]) or (required("both", tag) - expected) & names:
        raise ValueError("Unexpected platform assets")
    for name, asset in assets.items():
        if asset["size"] <= 0 or asset["state"] != "uploaded" or not (asset["digest"] or "").startswith("sha256:"):
            raise ValueError(f"Asset upload incomplete: {name}")
    record = api(f"releases/assets/{assets[marker(scope)]['id']}", "-H", "Accept: application/octet-stream")
    if any(record[key] != value for key, value in dict(tag=tag, scope=scope, commit=commit, run_id=run).items()):
        raise ValueError("Published scope record differs from confirmed run")
    inventory = {n: {"size": a["size"], "digest": a["digest"]} for n, a in assets.items()
                 if n not in (marker(scope), "SHA256SUMS.txt")}
    if record.get("assets") != inventory:
        raise ValueError("Published assets differ from the build inventory")
    extra = names - expected
    permitted = re.compile(r"MyWubi-components-[a-z0-9-]+-" + re.escape(tag[1:]) + r"-windows-x86_64[.]zip")
    if any("windows" not in SCOPES[scope] or
           (n != f"MyWubi-{tag[1:]}-delta.nupkg" and not permitted.fullmatch(n)) for n in extra):
        raise ValueError(f"Unexpected assets for {scope}: {sorted(extra)}")
    pages = api("releases?per_page=100", "--paginate", "--slurp")
    previous = baselines([r for page in pages for r in page if TAG.fullmatch(r["tag_name"])
                          and version(r["tag_name"]) < version(tag)])
    for platform in SCOPES[scope]:
        previous[platform] = tag
    entries = record["platforms"]
    if entries.keys() != previous.keys():
        raise ValueError("Missing or unexpected platform history")
    for platform, baseline in previous.items():
        entry = entries[platform]
        expected_commit = commit if baseline == tag else api("commits/" + baseline)["sha"]
        if entry["tag"] != baseline or entry["commit"] != expected_commit:
            raise ValueError(f"Wrong delivery baseline for {platform}")
    return release["html_url"], record["platforms"], sorted((n, a["size"]) for n, a in assets.items())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for field in ("tag", "scope", "commit", "run"):
        parser.add_argument(field)
    parser.add_argument("notes", type=Path)
    args = parser.parse_args()
    print(*verify(args.tag, args.scope, args.commit, args.run, args.notes.read_text(encoding="utf-8")), sep="\n")
