"""Derive a code-independent word lexicon from the pinned Apache-2.0 table."""
from pathlib import Path
import argparse
ROOT = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--check", action="store_true")
args = parser.parse_args()
words = {}
for line in (ROOT / "tables/pinyin.dict").read_text(encoding="utf-8").splitlines():
    parts = line.split("\t")
    if len(parts) != 3:
        continue
    _, word, weight = parts
    if 2 <= len(word) <= 16 and all("\u3400" <= c <= "\u9fff" or "\U00020000" <= c <= "\U000323af" for c in word):
        words[word] = max(words.get(word, 0), int(weight))
text = "# Derived from rime/rime-pinyin-simp 0c6861ef7420ee780270ca6d993d18d4101049d0.\n# Apache-2.0; original license: tables/licenses/rime-pinyin-simp-LICENSE.txt.\n" + "".join(f"{word}\t{weight}\n" for word, weight in sorted(words.items()))
target = ROOT / "core_engine/src/assistance/prediction_words.tsv"
if args.check:
    if target.read_bytes() != text.encode("utf-8"):
        raise SystemExit("prediction lexicon differs from pinned source")
else:
    target.write_bytes(text.encode("utf-8"))
print(f"{len(words)} words; {len(text.encode('utf-8'))} bytes")
