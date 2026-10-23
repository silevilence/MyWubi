"""Rebuild the bundled dictionaries from pinned, SHA-256 verified sources.

Python 3.10+; standard library only. Downloaded files stay under ignored target/.
No downloaded code is executed and no archive paths are extracted to disk.
"""

import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile
from urllib.request import urlopen


ROOT = Path(__file__).resolve().parents[1]
PINYIN_REV = "0c6861ef7420ee780270ca6d993d18d4101049d0"
PINYIN_BASE = f"https://raw.githubusercontent.com/rime/rime-pinyin-simp/{PINYIN_REV}"
SOURCES = {
    "pinyin_simp.dict.yaml": {
        "url": f"{PINYIN_BASE}/pinyin_simp.dict.yaml",
        "sha256": "e341598343a0f0f2035bb1aafc34a7f3bb7887deeecb3f60796262aaa2983e6b",
    },
    "pinyin-LICENSE": {
        "url": f"{PINYIN_BASE}/LICENSE",
        "sha256": "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
    },
    "pinyin-AUTHORS": {
        "url": f"{PINYIN_BASE}/AUTHORS",
        "sha256": "f4cff0fcbca4668ac449c24a53be547e162bc60cce63fdc5d5906801a452edc4",
    },
    "scowl-2020.12.07.tar.gz": {
        "url": "https://deb.debian.org/debian/pool/main/s/scowl/scowl_2020.12.07.orig.tar.gz",
        "sha256": "5587667caa20c4891390c2d42dbb4d5c4c3f41bee77af1457ece3ba23fb859cc",
    },
}


def source_bytes(name, cache, download):
    path = cache / name
    source = SOURCES[name]
    if path.exists():
        data = path.read_bytes()
    elif download:
        with urlopen(source["url"], timeout=60) as response:
            data = response.read()
    else:
        raise ValueError(f"Missing {path}; use --download or supply the pinned source")
    if hashlib.sha256(data).hexdigest() != source["sha256"]:
        raise ValueError(f"SHA-256 mismatch: {path}; refusing unverified input")
    if not path.exists():
        cache.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    return data


def pinyin_rows(data):
    text = data.decode("utf-8")
    body = text.split("...\n", 1)[1]
    rows = {}
    count = 0
    for line in body.splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        word, spelling, frequency = line.split("\t")
        syllables = spelling.split()
        if len(syllables) != len(word) or any(
            not re.fullmatch(r"[a-z]+", syllable) for syllable in syllables
        ):
            raise ValueError(f"Unsupported pinyin reading: {line!r}")
        weight = int(frequency)
        if not 0 <= weight <= 0xFFFFFFFF:
            raise ValueError(f"Invalid frequency: {line!r}")
        code = "'".join(syllables)
        if len(code.replace("'", "")) > 64:
            raise ValueError(f"Pinyin code exceeds 64 letters: {code}")
        key = (code, word)
        rows[key] = max(rows.get(key, 0), weight)
        count += 1
    return rows, {"source_rows": count, "duplicates_merged": count - len(rows)}


def english_rows(data):
    rows = {}
    selected = []
    excluded = 0
    root = "scowl-2020.12.07/"
    pattern = re.compile(r"final/(english|american|british)-words\.(\d+)")
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        copyright_text = archive.extractfile(root + "Copyright").read().decode("utf-8")
        for member in sorted(archive.getmembers(), key=lambda item: item.name):
            relative = member.name.removeprefix(root)
            pronoun_list = relative == "final/english-upper.10"
            match = pattern.fullmatch(relative)
            if not pronoun_list and (not match or int(match[2]) > 60):
                continue
            selected.append(relative)
            # SCOWLv1 word lists use ISO-8859-1, not UTF-8.
            words = archive.extractfile(member).read().decode("iso-8859-1")
            weight = (100 - (10 if pronoun_list else int(match[2]))) * 1000
            for word in words.splitlines():
                # Keep ordinary alphabetic words and the pronoun I. Do not
                # fabricate spellings by stripping accents or punctuation.
                if (pronoun_list and word != "I") or not re.fullmatch(r"[a-z]+|I", word) or len(word) > 32:
                    excluded += 1
                    continue
                key = (word.lower(), word)
                rows[key] = max(rows.get(key, 0), weight)
    if not selected or not rows:
        raise ValueError("SCOWL archive has no matching word lists")
    return rows, copyright_text, {"files": selected, "excluded_rows": excluded}


def table_bytes(table_type, max_length, notice, rows):
    header = (
        f"---\ntype: {table_type}\nwildcard_key: ''\n"
        "charset: abcdefghijklmnopqrstuvwxyz\n"
        f"max_code_len: {max_length}\n---\n"
    )
    comments = "".join(f"# {line.rstrip()}\n" if line.strip() else "#\n"
                       for line in notice.splitlines())
    body = "".join(f"{code}\t{word}\t{weight}\n"
                   for (code, word), weight in sorted(rows.items()))
    return (header + comments + body).encode("utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--download", action="store_true", help="fetch missing pinned sources")
    parser.add_argument("--check", action="store_true", help="compare without rewriting outputs")
    parser.add_argument("--source-dir", type=Path, default=ROOT / "target/dictionary-sources")
    args = parser.parse_args()
    sources = {name: source_bytes(name, args.source_dir, args.download) for name in SOURCES}
    pinyin, pinyin_stats = pinyin_rows(sources["pinyin_simp.dict.yaml"])
    english, copyright_text, english_stats = english_rows(sources["scowl-2020.12.07.tar.gz"])
    license_text = sources["pinyin-LICENSE"].decode("utf-8")
    authors = sources["pinyin-AUTHORS"].decode("utf-8")
    pinyin_notice = (
        "MyWubi pinyin dictionary; derived from Rime pinyin_simp / Android Pinyin IME.\n"
        f"Source: {PINYIN_BASE}/pinyin_simp.dict.yaml\n"
        "Modified for MyWubi: YAML header, code/word column order, syllable separators,\n"
        "deterministic sorting and duplicate merging (maximum original weight).\n"
        "Original numeric frequencies and all readings are retained. Use v for umlaut-u.\n"
        "Original source header:\n"
        + sources["pinyin_simp.dict.yaml"].decode("utf-8").split("---", 1)[0]
        + "\nUpstream AUTHORS:\n" + authors
        + "\nComplete upstream license follows; keep it when redistributing this file.\n"
        + license_text
    )
    english_notice = (
        "MyWubi English dictionary; derived from SCOWL 2020.12.07.\n"
        "Source: https://wordlist.aspell.net/other/\n"
        "Modified: english/american/british words at levels <=60; alphabetic lowercase\n"
        "words plus I from english-upper.10; exclude other uppercase entries, possessives,\n"
        "contractions, accented forms and separate proper-name lists.\n"
        "Code is lowercase, output preserves I. Duplicate pairs use the highest weight.\n"
        "Weight = (100 - lowest SCOWL level) * 1000, a coarse priority, NOT corpus frequency.\n"
        "Complete upstream Copyright follows; keep it when redistributing this file.\n"
        + copyright_text
    )
    outputs = {
        "pinyin.dict": table_bytes("pinyin", 64, pinyin_notice, pinyin),
        "english.dict": table_bytes("shape", 32, english_notice, english),
        "licenses/rime-pinyin-simp-LICENSE.txt": license_text.encode("utf-8"),
        "licenses/rime-pinyin-simp-AUTHORS.txt": authors.encode("utf-8"),
        "licenses/SCOWL-Copyright.txt": copyright_text.encode("utf-8"),
    }
    manifest = {
        "sources": SOURCES,
        "pinyin": {"entries": len(pinyin), **pinyin_stats},
        "english": {"entries": len(english), **english_stats},
        "outputs": {name: hashlib.sha256(data).hexdigest() for name, data in outputs.items()},
    }
    outputs["sources.json"] = (json.dumps(manifest, indent=2, ensure_ascii=False) + "\n").encode("utf-8")
    for name, data in outputs.items():
        path = ROOT / "tables" / name
        if args.check:
            if not path.exists() or path.read_bytes() != data:
                raise ValueError(f"Generated output differs: {path}")
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    print(f"{'Verified' if args.check else 'Generated'}: pinyin={len(pinyin)}, english={len(english)}")


if __name__ == "__main__":
    main()
