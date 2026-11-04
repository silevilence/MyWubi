"""Fit model-specific prediction admission on dev labels only.

Input JSON contains adapter, model_sha256, vocabulary_sha256, and samples with
id/group/split/prefix, acceptable (empty means abstain), and candidates{text,score}.
The caller must supply representative independently labelled positive AND negative
contexts. Reports do not certify a model; test labels never select thresholds.
"""
import argparse
import hashlib
import json
import math
import struct
from pathlib import Path


def f32(value):
    try:
        result = struct.unpack('!f', struct.pack('!f', value))[0]
    except (OverflowError, struct.error) as error:
        raise ValueError('score exceeds finite f32 range') from error
    if not math.isfinite(result):
        raise ValueError('score exceeds finite f32 range')
    return result


def margin_f32(value):
    rounded = f32(value)
    if rounded > value:
        bits = struct.unpack('!I', struct.pack('!f', rounded))[0]
        rounded = struct.unpack('!f', struct.pack('!I', bits - 1))[0]
    return rounded


def validate(data):
    if data['adapter'] not in ('gguf-continuation-v1', 'character-step-v1'):
        raise ValueError('unsupported score adapter')
    hashes = [data['model_sha256']]
    if data['adapter'] == 'character-step-v1':
        hashes.append(data.get('vocabulary_sha256', ''))
    elif data.get('vocabulary_sha256') is not None:
        raise ValueError('GGUF has no external vocabulary')
    if any(len(h) != 64 or any(c not in '0123456789abcdefABCDEF' for c in h) for h in hashes):
        raise ValueError('invalid artifact SHA-256')
    samples = data['samples']
    if not 1 <= len(samples) <= 10000:
        raise ValueError('sample count outside 1..10000')
    ids, groups, prefixes = set(), {}, {}
    for s in samples:
        if s['id'] in ids or s['split'] not in ('dev', 'test') or not s['group']:
            raise ValueError('invalid/duplicate sample')
        ids.add(s['id'])
        for mapping, key in ((groups, s['group']), (prefixes, s['prefix'])):
            if key in mapping and mapping[key] != s['split']:
                raise ValueError('cross-split leakage')
            mapping[key] = s['split']
        candidates = s['candidates']
        if len(candidates) > 64 or len({c['text'] for c in candidates}) != len(candidates):
            raise ValueError('invalid candidates')
        if any(not c['text'] or not math.isfinite(c['score']) for c in candidates):
            raise ValueError('invalid text or non-finite score')
        for c in candidates:
            f32(c['score'])
    dev = [s for s in samples if s['split'] == 'dev']
    if not any(s['acceptable'] for s in dev) or not any(not s['acceptable'] for s in dev):
        raise ValueError('dev requires positive and abstention contexts')
    if not any(s['split'] == 'test' for s in samples):
        raise ValueError('a separate test split is required')


def metrics(samples, minimum, margin):
    result = dict(samples=len(samples), recalled=0, shown=0, top1_correct=0,
                  candidates_shown=0, candidates_correct=0, negative=0, false_show=0)
    for s in samples:
        values = sorted((dict(c, score=f32(c['score'])) for c in s['candidates']), key=lambda c: -c['score'])
        acceptable = set(s['acceptable'])
        result['recalled'] += bool(acceptable.intersection(c['text'] for c in values))
        result['negative'] += not acceptable
        show = bool(values) and values[0]['score'] >= minimum
        show = show and (len(values) == 1 or values[0]['score'] - values[1]['score'] >= margin)
        if not show:
            continue
        admitted = [c for c in values if c['score'] >= minimum]
        result['shown'] += 1
        result['top1_correct'] += values[0]['text'] in acceptable
        result['false_show'] += not acceptable
        result['candidates_shown'] += len(admitted)
        result['candidates_correct'] += sum(c['text'] in acceptable for c in admitted)
    result['coverage'] = result['shown'] / len(samples) if samples else 0
    result['top1_precision'] = result['top1_correct'] / result['shown'] if result['shown'] else 0
    result['candidate_precision'] = result['candidates_correct'] / result['candidates_shown'] if result['candidates_shown'] else 0
    return result


def grid(values):
    values = sorted(set(values))
    return values if len(values) <= 32 else [values[i * (len(values) - 1) // 31] for i in range(32)]


def calibrate(data, precision=0.9, coverage=0.1):
    validate(data)
    if not (0 < precision <= 1 and 0 < coverage <= 1):
        raise ValueError('precision and coverage must be in (0, 1]')
    dev = [s for s in data['samples'] if s['split'] == 'dev']
    test = [s for s in data['samples'] if s['split'] == 'test']
    minimums = grid(f32(c['score']) for s in dev for c in s['candidates'])
    margins = [0.0]
    for s in dev:
        scores = sorted((f32(c['score']) for c in s['candidates']), reverse=True)
        if len(scores) > 1:
            gap = scores[0] - scores[1]
            if gap <= 3.4028234663852886e38:
                margins.append(margin_f32(gap))
    selected = None
    for minimum in minimums:
        for margin in grid(margins):
            m = metrics(dev, minimum, margin)
            if min(m['top1_precision'], m['candidate_precision']) < precision or m['coverage'] < coverage:
                continue
            rank = (m['top1_correct'], -m['false_show'], m['candidate_precision'], -minimum, -margin)
            if selected is None or rank > selected[0]:
                selected = (rank, minimum, margin, m)
    if selected is None:
        return dict(selected=None, reason='no dev threshold meets the requested constraints')
    _, minimum, margin, dev_metrics = selected
    return dict(selected=dict(minimum_score=minimum, minimum_margin=margin), dev=dev_metrics,
                test=metrics(test, minimum, margin),
                scope='Dev-selected thresholds; held-out test report only, not a product endorsement')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('scores', type=Path)
    parser.add_argument('--minimum-precision', type=float, default=0.9)
    parser.add_argument('--minimum-coverage', type=float, default=0.1)
    parser.add_argument('--policy-output', type=Path)
    args = parser.parse_args()
    raw = args.scores.read_bytes()
    data = json.loads(raw)
    result = calibrate(data, args.minimum_precision, args.minimum_coverage)
    if args.policy_output:
        if result['selected'] is None:
            parser.error('no qualifying policy; output was not written')
        test = result['test']
        if min(test['top1_precision'], test['candidate_precision']) < args.minimum_precision or test['coverage'] < args.minimum_coverage:
            parser.error('fixed policy failed held-out constraints; output was not written; collect new dev data, do not tune on test labels')
        policy = dict(version=1, adapter=data['adapter'], model_sha256=data['model_sha256'],
                      vocabulary_sha256=data.get('vocabulary_sha256'), **result['selected'],
                      evidence=json.dumps(dict(scores_sha256=hashlib.sha256(raw).hexdigest(), report=result), ensure_ascii=False))
        # Never silently overwrite an already reviewed policy.
        with args.policy_output.open('x', encoding='utf-8') as out:
            json.dump(policy, out, ensure_ascii=False, indent=2)
            out.write('\n')
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == '__main__':
    main()
