"""Emit the Android update contract from the signed APK's own manifest."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path


def metadata(badging, apk, tag, notes):
    package = re.search(r"package: name='([^']+)' versionCode='(\d+)' versionName='([^']+)'", badging)
    sdk = re.search(r"sdkVersion:'(\d+)'", badging)
    native = re.search(r"native-code: (.+)", badging)
    if not package or not sdk or not native:
        raise ValueError("APK manifest is incomplete")
    name, code, version = package.groups()
    if name != "org.mywubi" or not re.fullmatch(r"[vV]\d+\.\d+\.\d+", tag) or tag[1:] != version:
        raise ValueError("Release tag and APK identity must agree")
    abis = re.findall(r"'([^']+)'", native[1])
    if set(abis) != {"arm64-v8a", "x86_64"} or int(code) < 1 or int(sdk[1]) < 26:
        raise ValueError("Unsupported APK contract")
    return dict(schema=1, versionName=version, versionCode=int(code), minSdk=int(sdk[1]),
                abis=abis, notes=notes,
                apkUrl=f"https://github.com/silevilence/MyWubi/releases/download/{tag}/MyWubi-{version}-Android.apk",
                sha256=hashlib.sha256(apk.read_bytes()).hexdigest())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ("apk", "aapt", "tag", "notes", "output"):
        parser.add_argument(f"--{option}", required=True)
    args = parser.parse_args()
    badging = subprocess.check_output([args.aapt, "dump", "badging", args.apk], text=True, encoding="utf-8")
    data = metadata(badging, Path(args.apk), args.tag, Path(args.notes).read_text(encoding="utf-8"))
    Path(args.output).write_text(json.dumps(data, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
