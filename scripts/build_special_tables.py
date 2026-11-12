"""Build curated special resources offline. Output and ordering are versioned data."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
VERSION = "1.0.0"
NOTICE = (ROOT / "expressions/licenses/MyWubi-MIT.txt").read_text(encoding="utf-8")

# Original MyWubi curation: individual kana, not a romaji transliteration engine.
KANA = """
あ:a い:i う:u え:e お:o
か:ka き:ki く:ku け:ke こ:ko
さ:sa し:shi,si す:su せ:se そ:so
た:ta ち:chi,ti つ:tsu,tu て:te と:to
な:na に:ni ぬ:nu ね:ne の:no
は:ha ひ:hi ふ:fu,hu へ:he ほ:ho
ま:ma み:mi む:mu め:me も:mo
や:ya ゆ:yu よ:yo
ら:ra り:ri る:ru れ:re ろ:ro
わ:wa を:wo ん:n,nn
が:ga ぎ:gi ぐ:gu げ:ge ご:go
ざ:za じ:ji,zi ず:zu ぜ:ze ぞ:zo
だ:da ぢ:di,ji づ:du,zu で:de ど:do
ば:ba び:bi ぶ:bu べ:be ぼ:bo
ぱ:pa ぴ:pi ぷ:pu ぺ:pe ぽ:po ゔ:vu
ぁ:xa,la ぃ:xi,li ぅ:xu,lu ぇ:xe,le ぉ:xo,lo
っ:xtsu,xtu,ltsu ゃ:xya,lya ゅ:xyu,lyu ょ:xyo,lyo ゎ:xwa,lwa ゕ:xka ゖ:xke
ゐ:wi ゑ:we
""".split()


def kana_entries():
    # Explicit pair table keeps combining variants independent from their composed form.
    voiced = dict(zip("がぎぐげござじずぜぞだぢづでどばびぶべぼゔ", "かきくけこさしすせそたちつてとはひふへほう"))
    semivoiced = dict(zip("ぱぴぷぺぽ", "はひふへほ"))
    rows = []
    for offset, category in [(0, "平假名"), (0x60, "片假名")]:
        for pair in KANA:
            hira, readings = pair.split(":")
            text = chr(ord(hira) + offset)
            tags = ["假名", category]
            if hira in "ぁぃぅぇぉっゃゅょゎゕゖ":
                tags.append("小写假名")
            if hira in "ゐゑ":
                tags.append("历史假名")
            if hira in voiced:
                tags.append("浊音")
            if hira in semivoiced:
                tags.append("半浊音")
            rows.append((text, category + " " + readings.split(",")[0], tags, readings.split(",")))
        for pair in KANA:
            hira, readings = pair.split(":")
            base = voiced.get(hira) or semivoiced.get(hira)
            if base:
                mark = "\u3099" if hira in voiced else "\u309a"
                text = chr(ord(base) + offset) + mark
                rows.append((text, category + " " + readings.split(",")[0] + " 组合形式",
                             ["假名", category, "组合形式", "浊音" if hira in voiced else "半浊音"], readings.split(",")))
    return rows


def write_resource(kind, name, rows):
    entries = []
    for order, (text, label, tags, aliases) in enumerate(rows):
        entries.append(dict(text=text, name=label, triggers=[label, *tags], tags=tags,
                            aliases=aliases, order=order, sources=["mywubi-curated"]))
    assert len({row["text"] for row in entries}) == len(entries)
    document = dict(kind=kind, name=name, version=VERSION,
        sources=[dict(id="mywubi-curated", url="local:scripts/build_special_tables.py",
                      revision=VERSION, license="MIT", notice=NOTICE)], entries=entries)
    encoded = "# MyWubi Expressions 1\n" + json.dumps(document, ensure_ascii=False, indent=2) + "\n"
    target = ROOT / "expressions" / (kind + ".mwe")
    target.write_bytes(encoded.encode("utf-8"))
    coverage = dict(kind=kind, version=VERSION, count=len(entries),
                    sha256=hashlib.sha256(target.read_bytes()).hexdigest(),
                    outputs=[row["text"] for row in entries],
                    queries={q: [row["text"] for row in entries if q in row["aliases"]]
                             for q in sorted({q for row in entries for q in row["aliases"]})})
    (ROOT / "expressions" / (kind + "-coverage.json")).write_bytes(
        (json.dumps(coverage, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    print(kind, len(entries), coverage["sha256"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=["kana"])
    options = parser.parse_args()
    if options.kind == "kana":
        write_resource("kana", "MyWubi 假名字符", kana_entries())
