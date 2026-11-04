"""Development-only score calibration; test labels never select parameters.

The 38 authored cases have already been inspected during MI-05. The reported
test slice is a regression check, not a fresh estimate of product generalization.
This experiment trains no language model and uploads no data.
"""
import argparse
import json
import math
from pathlib import Path


def metrics(samples, runs, penalty, margin):
    out = dict(samples=len(samples), base_correct=0, top1=0,
               correct_to_wrong=0, wrong_to_correct=0, recalled=0)
    for sample in samples:
        candidates = sample['input']['candidates']
        base = candidates[0] if candidates else None
        picked = base
        values = runs[sample['id']].get('scores')
        if penalty is not None and values:
            scores = [(item['value'] - penalty * math.log1p(item['id']), item['id']) for item in values]
            scores.sort(key=lambda item: (-item[0], item[1]))
            if len(scores) == 1 or scores[0][0] - scores[1][0] >= margin:
                picked = candidates[scores[0][1]]
        before, after = base == sample['target'], picked == sample['target']
        out['base_correct'] += before
        out['top1'] += after
        out['correct_to_wrong'] += before and not after
        out['wrong_to_correct'] += after and not before
        out['recalled'] += sample['target'] in candidates
    return out


def assess(samples, comparison):
    dev = [s for s in samples if s['split'] == 'dev' and s['task'] == 'rank']
    test = [s for s in samples if s['split'] == 'test' and s['task'] == 'rank']
    result = {}
    for name in ['llama', 'onnx_authored_count_baseline']:
        runs = {r['sample']: r for r in comparison[name]['runs'] if r['pass'] == 0}
        trials = []
        for penalty, margin in [(None, 0), *[(p, m) for p in [0, .1, .3, 1, 3] for m in [0, .25, .5, 1, 2]]]:
            trials.append(dict(base_position_penalty=penalty, minimum_margin=margin,
                               dev=metrics(dev, runs, penalty, margin)))
        # Ties retain the base-only control; no test labels are consulted here.
        selected = max(trials, key=lambda t: (t['dev']['top1'], -t['dev']['correct_to_wrong']))
        result[name] = dict(trials=trials, selected_from_dev=selected,
                            test_regression=metrics(test, runs, selected['base_position_penalty'], selected['minimum_margin']))
    return dict(scope='Small previously inspected authored regression set; not product validation',
                training_samples=sum(s['split'] == 'train' for s in samples), models=result)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('comparison', type=Path)
    parser.add_argument('--samples', type=Path, default=Path(__file__).parents[1] / 'evaluation/samples.json')
    args = parser.parse_args()
    print(json.dumps(assess(json.loads(args.samples.read_text(encoding='utf-8')),
                            json.loads(args.comparison.read_text(encoding='utf-8-sig'))), ensure_ascii=False, indent=2))
