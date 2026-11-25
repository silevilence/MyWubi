"""MyWubi stable release identities and asset names (shared by CI and release tooling)."""
from fnmatch import fnmatchcase
import re

SCOPES = {"both": ("windows", "android"), "windows": ("windows",), "android": ("android",)}
TAG = re.compile(r"[vV](0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)")


def version(tag):
    match = TAG.fullmatch(tag)
    if not match:
        raise ValueError(f"Invalid stable tag: {tag}")
    return tuple(map(int, match.groups()))


def marker(scope):
    if scope not in SCOPES:
        raise ValueError(f"Unknown platform scope: {scope}")
    return f"release-scope-{scope}.json"


def platforms(names):
    markers = sorted(n for n in names if n.startswith("release-scope-"))
    if markers:
        for scope, selected in SCOPES.items():
            if markers == [marker(scope)]:
                return selected
        raise ValueError(f"Invalid scope assets: {markers}")
    # Releases predating scoped publication used these conventional asset names.
    return tuple(p for p, present in (
        ("windows", any(fnmatchcase(n, "releases.*.json") or n.endswith("-Setup.exe") for n in names)),
        ("android", any(n.endswith("-Android.apk") or n == "android-update.json" for n in names))
    ) if present)


def required(scope, tag):
    version(tag)
    marker(scope)
    v = tag[1:]
    names = set()
    if "windows" in SCOPES[scope]:
        names.update({"MyWubi-win-Setup.exe", "MyWubi-win-Portable.zip", f"MyWubi-{v}-full.nupkg",
                      "releases.win.json", "components.json", f"MyWubi-components-cpu-{v}-windows-x86_64.zip"})
    if "android" in SCOPES[scope]:
        for suffix in ("", "-full"):
            names.update({f"MyWubi-{v}-Android{suffix}.apk", f"MyWubi-{v}-Android{suffix}-native.json",
                          f"android-update{suffix}.json"})
    return names


def baselines(releases):
    """Most recent actual platform deliveries in global version order, including legacy releases."""
    stable = [r for r in releases if not r["draft"] and not r["prerelease"] and TAG.fullmatch(r["tag_name"])]
    latest = {}
    for release in sorted(stable, key=lambda r: version(r["tag_name"]), reverse=True):
        names = {a["name"] for a in release["assets"]}
        for platform in platforms(names):
            latest.setdefault(platform, release["tag_name"])
    return latest


def check_next(tag, scope, releases, code=None, previous_code=None):
    marker(scope)
    target = version(tag)
    public = [r for r in releases if not r["draft"] and not r["prerelease"] and TAG.fullmatch(r["tag_name"])]
    if any(version(r["tag_name"]) >= target for r in public):
        raise ValueError("Tag has already been published or is older than a public version")
    if scope != "both" and not any(marker("both") in {a["name"] for a in r["assets"]} for r in public):
        raise ValueError("Publish the dual-platform migration release first")
    if "android" in SCOPES[scope] and previous_code is not None and int(code) <= previous_code:
        raise ValueError("Android versionCode must increase")
