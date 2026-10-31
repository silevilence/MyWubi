"""Validate release versions and extract exact-version release notes (Python 3.11+)."""
import argparse
import json
import os
from pathlib import Path
import re
import tomllib


def prepare(root: Path, ref: str, destination: Path) -> dict:
    workspace = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]
    version = workspace["package"]["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("Expected a stable x.y.z workspace version")
    tag = ref.removeprefix("refs/tags/") if ref.startswith("refs/tags/") else f"V{version}"
    if not re.fullmatch(r"[vV]" + re.escape(version), tag):
        raise ValueError(f"Tag {tag} does not match workspace version {version}")
    for member in workspace["members"]:
        package = tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        if package["version"] not in (version, {"workspace": True}):
            raise ValueError(f"Version mismatch in {member}")
    gradle = (root / "android/app/build.gradle").read_text(encoding="utf-8")
    android_version = re.search(r"versionName\s+['\"]([^'\"]+)['\"]", gradle)
    if not android_version or android_version[1] != version:
        raise ValueError("Android versionName does not match workspace version")
    code = re.search(r"versionCode.*getOrElse\('([0-9]+)'\)", gradle)
    if not code or int(code[1]) <= 0:
        raise ValueError("Missing positive default Android versionCode")
    changelog = (root / "CHANGELOG.md").read_text(encoding="utf-8")
    sections = re.split(r"^##\s+", changelog, flags=re.MULTILINE)[1:]
    matches = [section.split("\n", 1)[1].strip() for section in sections
               if section.split("\n", 1)[0].strip().lower() == f"v{version}"]
    if len(matches) != 1 or not matches[0]:
        raise ValueError("Expected exactly one nonempty changelog entry for the release")
    destination.mkdir(parents=True, exist_ok=True)
    (destination / "release-notes.md").write_text(matches[0] + "\n", encoding="utf-8")
    metadata = {"version": version, "tag": tag, "version_code": code[1]}
    (destination / "version.json").write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return metadata


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ref", default=os.environ.get("GITHUB_REF", ""))
    parser.add_argument("--output-dir", type=Path, default=Path("release-metadata"))
    args = parser.parse_args()
    metadata = prepare(Path(__file__).resolve().parents[1], args.ref, args.output_dir)
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
            output.writelines(f"{key}={value}\n" for key, value in metadata.items())
    print(json.dumps(metadata))
