"""Prepare and publish a scoped GitHub Release using artifacts from the current Actions run."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess

from release_contract import SCOPES, baselines, check_next, marker, required, version
from release_metadata import prepare


def gh(*args):
    return subprocess.check_output(["gh", *map(str, args)], encoding="utf-8")


def api(path, *args):
    return json.loads(gh("api", "repos/{owner}/{repo}/" + path, *args))


def record(tag, scope, commit, run, code):
    pages = api("releases?per_page=100", "--paginate", "--slurp")
    history = [release for page in pages for release in page]
    previous = baselines(history)
    by_tag = {r["tag_name"]: r for r in history if not r["draft"]}
    platforms = {}
    for platform, baseline in previous.items():
        release = by_tag[baseline]
        entry = dict(tag=baseline, commit=api("commits/" + baseline)["sha"])
        if platform == "android":
            asset = next(a for a in release["assets"] if a["name"] == "android-update.json")
            manifest = api(f"releases/assets/{asset['id']}", "-H", "Accept: application/octet-stream")
            entry["versionCode"] = int(manifest["versionCode"])
        platforms[platform] = entry
    check_next(tag, scope, history, code, platforms.get("android", {}).get("versionCode"))
    if api("commits/" + tag)["sha"] != commit:
        raise ValueError("Remote tag no longer points to the build commit")
    for platform in SCOPES[scope]:
        platforms[platform] = dict(tag=tag, commit=commit)
        if platform == "android":
            platforms[platform]["versionCode"] = int(code)
    return dict(schema=1, tag=tag, scope=scope, commit=commit, run_id=run, platforms=platforms)


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def component_names(directory, tag):
    catalog = json.loads((directory / "components.json").read_text(encoding="utf-8-sig"))
    names = set()
    for entry in catalog:
        archive = entry["archive"]
        name = archive["path"]
        source = f"https://github.com/silevilence/MyWubi/releases/download/{tag}/{name}"
        if (not name.startswith("MyWubi-components-") or not name.endswith(f"-{tag[1:]}-windows-x86_64.zip")
                or "/" in name or "\\" in name or entry["source"] != source or name in names
                or entry["os"] != "windows" or entry["architecture"] != "x86_64"
                or entry["application"] != "=" + tag[1:]):
            raise ValueError(f"Component not bound to this Windows release: {name}")
        path = directory / name
        if not path.is_file() or path.stat().st_size != archive["bytes"] or digest(path) != archive["sha256"]:
            raise ValueError(f"Missing or corrupt authorized component: {name}")
        names.add(name)
    if f"MyWubi-components-cpu-{tag[1:]}-windows-x86_64.zip" not in names:
        raise ValueError("Missing CPU component in application catalog")
    return names


def assemble(root, identity, dist):
    scope, tag = identity["scope"], identity["tag"]
    dist.mkdir()
    for platform in SCOPES[scope]:
        directory = root / ("release-" + platform)
        expected = required(platform, tag)
        files = {p.name for p in directory.iterdir() if p.is_file() and p.stat().st_size}
        if not expected <= files:
            raise ValueError(f"Missing {platform} assets: {expected - files}")
        allowed = set(expected)
        if platform == "windows":
            allowed.update(component_names(directory, tag))
            allowed.add(f"MyWubi-{tag[1:]}-delta.nupkg")
        else:
            for suffix in ("", "-full"):
                manifest = json.loads((directory / f"android-update{suffix}.json").read_text())
                if manifest["versionCode"] != identity["platforms"]["android"]["versionCode"]:
                    raise ValueError("Android APK versionCode differs from the recorded version")
        if files - allowed or any(p.name not in files for p in directory.iterdir()):
            raise ValueError(f"Unexpected or empty {platform} artifact")
        for name in files:
            shutil.copyfile(directory / name, dist / name)
    (dist / marker(scope)).write_text(json.dumps(identity, indent=2) + "\n")
    hashes = [f"{digest(p)}  {p.name}" for p in sorted(dist.iterdir())]
    (dist / "SHA256SUMS.txt").write_text("\n".join(hashes) + "\n")


def remaining(assets, dist):
    pending = {p.name for p in dist.iterdir()}
    for asset in assets:
        name = asset["name"]
        if (name not in pending or asset["state"] != "uploaded"
                or asset["digest"] != "sha256:" + digest(dist / name)
                or asset["size"] != (dist / name).stat().st_size):
            raise ValueError(f"Draft contains a foreign, incomplete or different asset: {name}")
        pending.remove(name)
    return sorted(pending)


def publish(identity, notes, dist):
    tag = identity["tag"]
    stamp = json.dumps({key: identity[key] for key in ("tag", "scope", "commit", "run_id")}, sort_keys=True)
    body = notes.rstrip() + f"\n\n<!-- mywubi-release {stamp} -->"
    drafts = [r for page in api("releases?per_page=100", "--paginate", "--slurp") for r in page if r["tag_name"] == tag]
    if drafts:
        draft = drafts[0]
    else:
        draft = api("releases", "-f", f"tag_name={tag}", "-f", f"target_commitish={identity['commit']}",
                    "-f", f"name=MyWubi {tag}", "-f", f"body={body}", "-F", "draft=true")
    if not draft["draft"] or draft["body"] != body or draft["target_commitish"] != identity["commit"]:
        raise ValueError("Release is public or draft identity differs; recover the original run")
    def assets():
        return [a for page in api(f"releases/{draft['id']}/assets?per_page=100", "--paginate", "--slurp") for a in page]
    for name in remaining(assets(), dist):
        gh("release", "upload", tag, dist / name)
    if remaining(assets(), dist):
        raise ValueError("Draft upload incomplete")
    api(f"releases/{draft['id']}", "-X", "PATCH", "-F", "draft=false", "-f", "make_latest=true")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("step", choices=["prepare", "publish"])
    args = parser.parse_args()
    tag, scope = os.environ["RELEASE_TAG"], os.environ["RELEASE_SCOPE"]
    version(tag)
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    root = Path("release-metadata") if args.step == "prepare" else Path("artifacts/release-metadata")
    if args.step == "prepare":
        metadata = prepare(Path.cwd(), "refs/tags/" + tag, root, scope)
    else:
        metadata = json.loads((root / "version.json").read_text())
    identity = record(tag, scope, commit, os.environ["GITHUB_RUN_ID"], metadata["version_code"])
    path = root / marker(scope)
    if args.step == "prepare":
        path.write_text(json.dumps(identity, indent=2) + "\n")
        with open(os.environ["GITHUB_OUTPUT"], "a") as out:
            for key, value in dict(tag=tag, scope=scope, commit=commit, version=tag[1:]).items():
                print(f"{key}={value}", file=out)
    else:
        if identity != json.loads(path.read_text()):
            raise ValueError("Release history or identity changed during the build")
        dist = Path("dist")
        assemble(root.parent, identity, dist)
        publish(identity, (root / "release-notes.md").read_text(encoding="utf-8"), dist)


if __name__ == "__main__":
    main()
