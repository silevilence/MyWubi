"""Freeze a source-separated generation holdout. Never used to tune parameters.

All UD GSDSimp wiki material belongs to holdout; the development source is the
separate MyWubi authored probe set. Text/annotations remain CC BY-SA 4.0.
"""
import hashlib
import json
from pathlib import Path
import urllib.request

REVISION = '4231dfd59866fa5999ad4a6bc1fdecd7985b3b59'
BASE = f'https://raw.githubusercontent.com/UniversalDependencies/UD_Chinese-GSDSimp/{REVISION}/'
ROOT = Path(__file__).resolve().parents[1]


def chinese(text):
    return bool(text) and all('\u3400' <= c <= '\u9fff' for c in text)


def main():
    cache = ROOT.parent / 'target/generation-corpus'
    cache.mkdir(parents=True, exist_ok=True)
    hashes = {}
    for name in ('README.md', 'LICENSE.txt', 'zh_gsdsimp-ud-test.conllu'):
        path = cache / name
        if not path.exists():
            path.write_bytes(urllib.request.urlopen(BASE + name, timeout=30).read())
        hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    corpus = (cache / 'zh_gsdsimp-ud-test.conllu').read_text(encoding='utf-8')
    sentences = {}
    excluded = {'too_short_or_long': 0, 'no_bounded_content_word': 0, 'duplicate_sentence': 0}
    for block in corpus.split('\n\n'):
        lines = block.splitlines()
        sid = next((line.split(' = ', 1)[1] for line in lines if line.startswith('# sent_id = ')), None)
        tokens = [line.split('\t') for line in lines if line and line[0].isdigit() and line.split('\t')[0].isdigit()]
        if not sid or not tokens:
            continue
        text = ''.join(token[1] for token in tokens)
        if not 12 <= len(text) <= 256:
            excluded['too_short_or_long'] += 1
            continue
        digest = hashlib.sha256(text.encode()).hexdigest()
        if digest in sentences:
            excluded['duplicate_sentence'] += 1
            continue
        prefix = ''
        choices = []
        for token in tokens:
            word = token[1]
            if 6 <= len(prefix) <= 48 and 1 <= len(word) <= 6 and chinese(word) and token[3] in ('NOUN', 'PROPN', 'VERB', 'ADJ'):
                choices.append((prefix, word))
            prefix += word
        if not choices:
            excluded['no_bounded_content_word'] += 1
            continue
        sentences[digest] = (sid, text, choices)
    rows = []
    for index, (digest, (sid, text, choices)) in enumerate(sorted(sentences.items())[:24]):
        prefix, word = choices[0]
        kind = 'next_word'
        if index % 3 == 0 and len(word) > 1:
            prefix += word[0]
            word = word[1:]
            kind = 'word_remainder'
        source = {'repository': 'UniversalDependencies/UD_Chinese-GSDSimp', 'revision': REVISION,
                  'file': 'zh_gsdsimp-ud-test.conllu', 'sent_id': sid, 'sentence_sha256': digest,
                  'license': 'CC-BY-SA-4.0'}
        rows.append(dict(id=f'holdout-{index:02}', split='holdout', source=source, kind=kind,
                         prefix=prefix, references=[word], should_stop=False))
        if index < 8 and text.endswith(('。', '！', '？')):
            rows.append(dict(id=f'stop-{index:02}', split='holdout', source=source, kind='sentence_stop',
                             prefix=text, references=[], should_stop=True))
    rows.append(dict(id='empty', split='holdout', source={'repository': 'empty-input-contract', 'license': 'MIT'},
                     kind='no_context', prefix='', references=[], should_stop=True))
    output = ROOT / 'evaluation/generation-holdout.json'
    output.write_text(json.dumps({'source_url': BASE, 'source_hashes': hashes,
        'attribution': 'UD Chinese GSDSimp: Peng Qi, Koichi Yasuoka; converted from UD Chinese GSD, wiki genre. CC BY-SA 4.0. Original underlying wiki rights remain with their authors.',
        'split_rule': 'Entire external corpus source is holdout, none used for prompt or threshold tuning. Deduplicated full sentences; deterministic SHA-256 order. Authored development examples are a separate source.',
        'exclusions': excluded, 'eligible_sentences': len(sentences), 'selected_sentences': min(24, len(sentences)),
        'samples': rows}, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(f'{len(rows)} holdout cases; {len(sentences)} eligible source sentences')


if __name__ == '__main__':
    main()
