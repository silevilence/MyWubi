"""Rebuild expression resources offline from pinned source bytes (Python 3.10+).

No network calls, downloaded code execution, Unicode normalization or output trimming.
Use --check in CI to verify sources, complete coverage and reproducible output.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
RESOURCES = ROOT / "expressions"
MAGIC = "# MyWubi Expressions 1\n"


def json_text(value):
    return json.dumps(value, ensure_ascii=False, indent=2) + "\n"


def document_text(document):
    """Keep one complete entry per line so dataset diffs remain reviewable."""
    header = {key: value for key, value in document.items() if key != "entries"}
    lines = json_text(header).rstrip()[:-1].rstrip()
    return MAGIC + lines + ',\n  "entries": [\n' + ",\n".join(
        "    " + json.dumps(entry, ensure_ascii=False) for entry in document["entries"]
    ) + "\n  ]\n}\n"


def source(name):
    manifest = json.loads((RESOURCES / "sources.json").read_text(encoding="utf-8"))
    data = (RESOURCES / "sources" / name).read_bytes()
    expected = manifest[name]
    if hashlib.sha256(data).hexdigest() != expected["sha256"] or len(data) != expected["bytes"]:
        raise ValueError(f"Source changed: {name}; refusing unverified bytes")
    return data.decode("utf-8")


def emoji_rows(text):
    group = subgroup = ""
    rows = []
    for line in text.splitlines():
        if line.startswith("# group: "):
            group = line.removeprefix("# group: ")
        elif line.startswith("# subgroup: "):
            subgroup = line.removeprefix("# subgroup: ")
        elif line and not line.startswith("#"):
            fields, comment = line.split("#", 1)
            points, qualification = fields.split(";")
            text = "".join(chr(int(point, 16)) for point in points.split())
            name = re.split(r" E[0-9.]+ ", comment.strip(), maxsplit=1)[1]
            rows.append((text, qualification.strip(), name, group, subgroup))
    return rows


def point_key(text):
    return " ".join(f"{ord(c):04X}" for c in text)


def emoji():
    rows = emoji_rows(source("emoji-test-18.0.txt"))
    names, keywords = {}, {}
    for name in ("annotations-zh-49.xml", "annotationsDerived-zh-49.xml"):
        for annotation in ET.fromstring(source(name)).iter("annotation"):
            text = annotation.attrib["cp"]
            if annotation.attrib.get("type") == "tts":
                names[text] = annotation.text
            else:
                keywords[text] = [word.strip() for word in annotation.text.split("|")]
    canonical = {}
    for text, status, *_ in rows:
        if status in ("fully-qualified", "component"):
            key = text.replace("\ufe0f", "")
            if key in canonical:
                raise ValueError(f"Ambiguous canonical sequence: {point_key(text)}")
            canonical[key] = text
    aliases = defaultdict(list)
    for text, status, *_ in rows:
        if status not in ("fully-qualified", "component"):
            aliases[canonical[text.replace("\ufe0f", "")]].append(text)
    entries = []
    for text, status, _, _, _ in rows:
        if status not in ("fully-qualified", "component"):
            continue
        key = text.replace("\ufe0f", "")
        name = names[key]
        terms = list(dict.fromkeys([name, *keywords.get(key, [])]))
        if not any(re.search(r"[\u3400-\u9fff]", term) for term in terms):
            raise ValueError(f"Missing Chinese trigger: {point_key(text)}")
        entries.append(dict(text=text, name=name, triggers=terms, tags=keywords.get(key, []),
                            aliases=aliases[text], order=len(entries), sources=["unicode-18", "cldr-49"]))
    notice = source("UNICODE-49-LICENSE.txt")
    doc = dict(kind="emoji", name="Unicode 18.0 中文 Emoji 全集", version="18.0-cldr49-1", sources=[
        dict(id="unicode-18", url="https://www.unicode.org/Public/18.0.0/emoji/emoji-test.txt",
             revision="18.0", license="Unicode-3.0", notice="Copyright © 2026 Unicode, Inc.\n" + notice),
        dict(id="cldr-49", url="https://github.com/unicode-org/cldr/tree/release-49",
             revision="release-49", license="Unicode-3.0", notice=notice),
    ], entries=entries)
    expected = {text for text, status, *_ in rows if status in ("fully-qualified", "component")}
    actual = {entry["text"] for entry in entries}
    previous = {text for text, status, *_ in emoji_rows(source("emoji-test-17.0.txt"))
                if status in ("fully-qualified", "component")}
    encoded = document_text(doc)
    report = dict(unicode="18.0", cldr="49", expected=len(expected), actual=len(entries),
                  missing=sorted(map(point_key, expected - actual)), extra=sorted(map(point_key, actual - expected)),
                  chinese_trigger_coverage=sum(any(re.search(r"[\u3400-\u9fff]", term) for term in e["triggers"]) for e in entries),
                  qualifications=dict(Counter(status for _, status, *_ in rows)),
                  aliases=sum(map(len, aliases.values())),
                  groups=dict(Counter(group for _, status, _, group, _ in rows if status in ("fully-qualified", "component"))),
                  added_since_17=[dict(sequence=point_key(e["text"]), name=e["name"]) for e in entries if e["text"] not in previous],
                  removed_since_17=sorted(map(point_key, previous-actual)),
                  output_sha256=hashlib.sha256(encoded.encode()).hexdigest())
    if report["missing"] or report["extra"] or report["chinese_trigger_coverage"] != len(expected):
        raise ValueError("Incomplete Emoji resource")
    return {RESOURCES / "emoji.mwe": encoded, RESOURCES / "emoji-coverage.json": json_text(report)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Verify without modifying outputs")
    args = parser.parse_args()
    outputs = emoji()
    for path, text in outputs.items():
        if args.check:
            if not path.exists() or path.read_bytes() != text.encode("utf-8"):
                raise ValueError(f"Generated output differs: {path}")
        else:
            path.write_bytes(text.encode("utf-8"))
    print(f"{'Verified' if args.check else 'Generated'} {len(outputs)} expression artifacts offline")


if __name__ == "__main__":
    main()
