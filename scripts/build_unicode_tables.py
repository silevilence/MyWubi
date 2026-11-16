"""Audit and build categorized Unicode 18 resources from pinned, local UCD files.

No downloads or normalization. --check verifies bytes and coverage reproducibly.
S/P means Unicode General_Category, not all characters that look like symbols.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
from build_expressions import document_text
from build_special_tables import math_entries, punctuation_entries, symbol_entries

ROOT = Path(__file__).resolve().parents[1]
DIR = ROOT / 'expressions'
CATEGORIES = {
    'greek': '希腊字母', 'cyrillic': '西里尔字母', 'latin': '拼音与拉丁',
    'phonetic': '音标', 'enclosed': '圈号与括号', 'numbers': '数字形式',
    'arrows': '箭头', 'currency': '货币', 'units': '单位', 'shapes': '几何图形',
    'box_drawing': '制表与方块', 'braille': '盲文', 'music': '音乐符号',
    'diacritics': '组合附加符', 'math': '数学符号', 'punctuation': '标点', 'symbols': '其他符号',
}
PINNED = {
    'UnicodeData': '0736451DE439AE7BAF1425136617DA495E09EE5AFBE6E394374DB7009EA08950',
    'Blocks': 'A58F8D322F3C5E254F9F97B1CBF76A454A7E02DE6AC35619A5E4F27AAABFD553',
    'Scripts': '0071FD81B6AEAE25F6E8BCE8EFEC3066A6476A91B49BDB2F52DC76E817862A6A',
    'DerivedCoreProperties': '09C928886A178FCAFD93C29E4BD59073A058E5A100B716D425CB563AB50F68C9',
}


def source(name):
    data = (DIR / 'sources' / f'{name}-18.0.txt').read_bytes()
    assert hashlib.sha256(data).hexdigest().upper() == PINNED[name], name
    return data.decode('utf-8')


def properties(name, wanted=None):
    result = {}
    for line in source(name).splitlines():
        line = line.split('#')[0].strip()
        if not line:
            continue
        fields = [v.strip() for v in line.split(';')]
        points, value = fields[:2]
        if wanted and value != wanted:
            continue
        ends = points.split('..')
        result.update((c, value) for c in range(int(ends[0], 16), int(ends[-1], 16)+1))
    return result


def classify(cp, name, gc, block, script):
    if 'Braille' in block: return 'braille'
    if 'Musical' in block or block == 'Ancient Greek Musical Notation' or 0x2669 <= cp <= 0x266f: return 'music'
    if 'Arrows' in block or {'ARROW', 'ARROWHEAD'}.intersection(name.split()): return 'arrows'
    if 'Enclosed' in block: return 'enclosed'
    if block in ('Box Drawing', 'Block Elements', 'Symbols for Legacy Computing', 'Symbols for Legacy Computing Supplement'): return 'box_drawing'
    if 'Geometric Shapes' in block: return 'shapes'
    if gc == 'Sc': return 'currency'
    if block == 'CJK Compatibility' or name.startswith('DEGREE ') or name in ('OHM SIGN', 'KELVIN SIGN', 'ANGSTROM SIGN'): return 'units'
    if gc.startswith('N') or block in ('Superscripts and Subscripts', 'Number Forms'): return 'numbers'
    if gc == 'Sm' or 'Mathematical' in block: return 'math'
    if block.startswith(('IPA Extensions', 'Phonetic Extensions', 'Spacing Modifier Letters')): return 'phonetic'
    if block.startswith(('Combining Diacritical Marks', 'Combining Half Marks', 'Combining Marks for Symbols')): return 'diacritics'
    if gc.startswith('L') and script in ('Greek', 'Cyrillic', 'Latin'):
        return {'Greek':'greek', 'Cyrillic':'cyrillic', 'Latin':'latin'}[script]
    if gc.startswith('P'): return 'punctuation'
    if gc.startswith('S'): return 'symbols'
    return None


def build():
    blocks, scripts = properties('Blocks'), properties('Scripts')
    invisible = properties('DerivedCoreProperties', 'Default_Ignorable_Code_Point')
    records = []
    for line in source('UnicodeData').splitlines():
        fields = line.split(';')
        cp = int(fields[0], 16)
        if fields[1].startswith('<'): continue  # CJK/Hangul range sentinels, control labels, surrogates.
        records.append((cp, fields[1], fields[2]))
    old = set()
    for entries in (math_entries(), punctuation_entries(), symbol_entries()):
        old.update(text for text, *_ in entries)
    for kind in ('emoji', 'kaomoji', 'kana'):
        old.update(e['text'] for e in json.loads((DIR/f'{kind}.mwe').read_text(encoding='utf-8').split('\n',1)[1])['entries'])
    tables = {kind: {} for kind in CATEGORIES}
    # Preserve all established names, aliases, ordering and multi-character outputs.
    for kind, rows in [('math',math_entries()),('punctuation',punctuation_entries()),('symbols',symbol_entries())]:
        for text, label, tags, aliases in rows:
            chinese = [alias for alias in aliases if any('\u4e00' <= c <= '\u9fff' for c in alias)]
            tables[kind][text] = dict(text=text,name=label,triggers=list(dict.fromkeys([label,*tags,*chinese])),tags=tags,aliases=aliases)
    curated = {text: entry for entries in tables.values() for text, entry in entries.items()}
    before, expected, excluded = Counter(), Counter(), Counter()
    pinyin = dict(zip('āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜ', [f'{v}{tone}' for v in ('a','e','i','o','u','v') for tone in range(1,5)]))
    for cp, name, gc in records:
        block = blocks.get(cp,'No_Block')
        kind = classify(cp,name,gc,block,scripts.get(cp,'Unknown'))
        if cp in invisible:
            if kind: excluded['default_ignorable'] += 1
            continue
        if kind is None:
            excluded[gc] += 1
            continue
        assert not chr(cp).isspace() and gc[0] != 'C', f'Unexpected non-display entry U+{cp:04X} {name}'
        expected[kind] += 1
        text = chr(cp)
        before[kind] += int(text in old)
        aliases = [name.lower(),f'U+{cp:04X}',f'{cp:04X}']
        label = f'{CATEGORIES[kind]} U+{cp:04X} {text}'
        if text.lower() in pinyin:
            sound = pinyin[text.lower()]
            prefix = 'capital ' if text.isupper() else ''
            aliases += [prefix+sound, prefix+sound.replace('v','ü')]
            label = f'{"大写" if text.isupper() else ""}拼音 {sound[0]} {sound[-1]}声 {text}'
        # Official English names remain visible as metadata, without claiming invented translations.
        entry = dict(text=text,name=label,triggers=[label,CATEGORIES[kind]],tags=[CATEGORIES[kind],block],aliases=aliases)
        if text in curated:
            original = curated[text]
            entry['name'] = original['name']
            for field in ('triggers', 'tags', 'aliases'):
                entry[field] = list(dict.fromkeys([*original[field], *entry[field]]))
        tables[kind].setdefault(text,entry)
    notice = (DIR/'sources/UNICODE-49-LICENSE.txt').read_text(encoding='utf-8')
    own_notice = (DIR/'licenses/MyWubi-MIT.txt').read_text(encoding='utf-8')
    outputs, report = {}, {'unicode':'18.0.0','sources':PINNED,'categories':{},'excluded_categories':dict(excluded)}
    report['scope'] = 'All assigned, non-default-ignorable P*, S* and N*; Latin/Greek/Cyrillic letters, phonetic and selected combining blocks. No arbitrary scripts, controls, unassigned, private-use or invisible formatting.'
    all_texts = set(old)
    for kind, entries in tables.items():
        # Browsing should expose familiar glyphs before historical extensions.
        if kind not in ('math', 'punctuation', 'symbols'):
            preferred = {
                'latin': 'āáǎàēéěèīíǐìōóǒòūúǔùǖǘǚǜüĀÁǍÀĒÉĚÈĪÍǏÌŌÓǑÒŪÚǓÙǕǗǙǛÜ',
                'phonetic': 'əɪʊʌɒæɛɜɑɔʃʒθðŋɡ',
                'currency': '¥￥$€£₩₹₽₿',
                'box_drawing': '─│┌┐└┘├┤┬┴┼╬▀▄█',
                'music': '♩♪♫♬♭♮♯',
            }.get(kind, '')
            first = list(dict.fromkeys([*preferred, *curated]))
            rank = {text: index for index, text in enumerate(first)}
            ordered = sorted(entries, key=lambda text: (
                0 if text in rank else 2 if kind == 'latin' and text.isascii() else 1,
                rank.get(text, ord(text))))
            entries = {text: entries[text] for text in ordered}
        for order, entry in enumerate(entries.values()):
            entry.update(order=order,sources=['unicode-18','mywubi-curated'])
        document = dict(kind=kind,name=f'Unicode 18 {CATEGORIES[kind]}',version='18.0.0-1',sources=[
            dict(id='unicode-18',url='https://www.unicode.org/Public/18.0.0/ucd/UnicodeData.txt',revision='18.0.0',license='Unicode-3.0',notice=notice),
            dict(id='mywubi-curated',url='local:scripts/build_unicode_tables.py',revision='1',license='MIT',notice=own_notice)],entries=list(entries.values()))
        encoded = document_text(document)
        outputs[DIR/f'{kind}.mwe'] = encoded
        if kind in ('math', 'punctuation', 'symbols'):
            # An upgrade can offer the expanded builtin without overwriting an edited old file.
            outputs[DIR/f'{kind}-unicode18.mwe'] = encoded
            outputs[DIR/f'{kind}-coverage.json'] = json.dumps(dict(
                kind=kind, version=document['version'], count=len(entries),
                sha256=hashlib.sha256(encoded.encode()).hexdigest(),
                outputs=list(entries), queries={q:[e['text'] for e in entries.values() if q in e['aliases']]
                    for q in sorted({q for e in entries.values() for q in e['aliases']})}), ensure_ascii=False,indent=2)+'\n'
        report['categories'][kind] = dict(label=CATEGORIES[kind],ucd_expected=expected[kind],previously_available=before[kind],previously_missing=expected[kind]-before[kind],entries=len(entries),sha256=hashlib.sha256(encoded.encode()).hexdigest())
        all_texts.update(entries)
    report['missing_symbols_or_punctuation'] = [f'U+{cp:04X}' for cp,_,gc in records if gc[0] in 'SP' and cp not in invisible and chr(cp) not in all_texts]
    assert not report['missing_symbols_or_punctuation']
    report['missing_numbers'] = [f'U+{cp:04X}' for cp,_,gc in records if gc[0] == 'N' and cp not in invisible and chr(cp) not in all_texts]
    assert not report['missing_numbers']
    report['examples'] = {c:dict(before=c in old,after=c in all_texts,categories=[k for k,v in tables.items() if c in v]) for c in 'α①āǚⒶ⓿Ⅷ⅝↯₿㎏╬⣿♯ʃЖ𝒜'}
    outputs[DIR/'unicode-coverage.json'] = json.dumps(report,ensure_ascii=False,indent=2)+'\n'
    return outputs


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    for path,text in build().items():
        encoded=text.encode('utf-8')
        if args.check:
            assert path.read_bytes()==encoded, f'Out of date: {path}'
        else:
            path.write_bytes(encoded)
    print('Unicode tables and coverage verified' if args.check else 'Unicode tables and coverage generated')
