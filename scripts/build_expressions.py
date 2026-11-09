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
import unicodedata
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
    if name.startswith("kaomoji"):
        manifest = json.loads((RESOURCES / "kaomoji-sources.json").read_text(encoding="utf-8"))
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


def category_mapping():
    mapping = {}
    for line in (RESOURCES / "kaomoji-categories.tsv").read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#"):
            continue
        categories, terms = line.split("\t")
        for category in categories.split():
            if category in mapping:
                raise ValueError(f"Duplicate category translation: {category}")
            mapping[category] = terms.split("|")
    return mapping


def exclusion_reason(text):
    if not text.strip():
        return "blank"
    if len(text) > 512:
        return "over_512_codepoints"
    if any(c in "\n\r\u2028\u2029" for c in text):
        return "multiline"
    if any(unicodedata.category(c) == "Cc" for c in text):
        return "control_character"
    if re.search(r"&(?:#\d+|#x[\da-fA-F]+|[A-Za-z]+);", text):
        return "html_entity_residue"
    return None


def kaomoji():
    mapping = category_mapping()
    first = json.loads(source("kaomojiya-kaomoji.json"))
    second = json.loads(source("kaomojikan-kaomoji.json"))
    inputs = [
        ("kaomojiya", category, index, text, [category])
        for category, texts in first.items() for index, text in enumerate(texts)
    ] + [
        ("kaomojikan", item["slug"], index, item["text"], item["categories"])
        for index, item in enumerate(second)
    ]
    counts = {name: dict(raw=0, accepted_occurrences=0, excluded_occurrences=0, unique_outputs=0, categories={})
              for name in ("kaomojiya", "kaomojikan")}
    unique = defaultdict(set)
    entries, excluded = {}, []
    for source_id, location, index, text, categories in inputs:
        counts[source_id]["raw"] += 1
        for category in categories:
            if category not in mapping:
                raise ValueError(f"Untranslated category: {category}")
            counts[source_id]["categories"][category] = counts[source_id]["categories"].get(category, 0) + 1
        reason = exclusion_reason(text)
        if reason:
            counts[source_id]["excluded_occurrences"] += 1
            excluded.append(dict(source=source_id, location=location, index=index, text=text, reason=reason))
            continue
        counts[source_id]["accepted_occurrences"] += 1
        unique[source_id].add(text)
        terms = list(dict.fromkeys(term for category in categories for term in mapping[category]))
        entry = entries.setdefault(text, dict(text=text, name=terms[0], triggers=[], tags=[], aliases=[],
                                             order=len(entries), sources=[]))
        entry["triggers"] = list(dict.fromkeys([*entry["triggers"], *terms]))
        entry["tags"] = entry["triggers"].copy()
        if source_id not in entry["sources"]:
            entry["sources"].append(source_id)
    for source_id in counts:
        counts[source_id]["unique_outputs"] = len(unique[source_id])
        counts[source_id]["duplicates_within_source"] = counts[source_id]["accepted_occurrences"] - len(unique[source_id])
    source_manifest = json.loads((RESOURCES / "kaomoji-sources.json").read_text(encoding="utf-8"))
    repositories = {"kaomojiya": "kaomojiya-collection/kaomoji-collection", "kaomojikan": "kaomojikan/kaomoji-data"}
    sources = [dict(id=source_id, url="https://github.com/" + repo,
                    revision=source_manifest[source_id + "-kaomoji.json"]["revision"],
                    license="MIT", notice=source(source_id + "-LICENSE"))
               for source_id, repo in repositories.items()]
    sources.append(dict(id="mywubi-zh", url="https://github.com/silevilence/MyWubi",
                        revision="kaomoji-categories-v1", license="MIT",
                        notice=(RESOURCES / "licenses/MyWubi-MIT.txt").read_text(encoding="utf-8")))
    for entry in entries.values():
        entry["sources"].append("mywubi-zh")
    doc = dict(kind="kaomoji", name="颜文字中文分类合集", version="2026.11.09-1", sources=sources, entries=list(entries.values()))
    encoded = document_text(doc)
    report = dict(sources=counts, total_raw=len(inputs), total_unique=len(entries),
                  cross_source_duplicates=len(unique["kaomojiya"] & unique["kaomojikan"]),
                  excluded_by_reason=dict(Counter(item["reason"] for item in excluded)),
                  translated_categories=len(mapping),
                  chinese_trigger_coverage=sum(any(re.search(r"[\u3400-\u9fff]", t) for t in e["triggers"]) for e in entries.values()),
                  output_sha256=hashlib.sha256(encoded.encode()).hexdigest())
    if report["chinese_trigger_coverage"] != len(entries):
        raise ValueError("Kaomoji missing Chinese triggers")
    return {RESOURCES / "kaomoji.mwe": encoded,
            RESOURCES / "kaomoji-coverage.json": json_text(report),
            RESOURCES / "kaomoji-excluded.json": json_text(excluded)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="Verify without modifying outputs")
    parser.add_argument("--kind", choices=("all", "emoji", "kaomoji"), default="all")
    args = parser.parse_args()
    outputs = {}
    if args.kind in ("all", "emoji"):
        outputs.update(emoji())
    if args.kind in ("all", "kaomoji"):
        outputs.update(kaomoji())
    for path, text in outputs.items():
        if args.check:
            if not path.exists() or path.read_bytes() != text.encode("utf-8"):
                raise ValueError(f"Generated output differs: {path}")
        else:
            path.write_bytes(text.encode("utf-8"))
    print(f"{'Verified' if args.check else 'Generated'} {len(outputs)} expression artifacts offline")


if __name__ == "__main__":
    main()
