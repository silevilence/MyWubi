"""Summarize probe-pipeline evidence without turning missing metrics into zeroes."""
import argparse
import json
import math
from pathlib import Path


def read(path):
    return [json.loads(line) for line in path.read_text(encoding='utf-8-sig').splitlines() if line.strip()]


def percentile(values, fraction):
    if not values:
        return None
    return sorted(values)[max(0, math.ceil(len(values) * fraction) - 1)]


def resource_summary(rows):
    result = {}
    for key in ('working_set', 'private_bytes', 'cpu_one_core_percent', 'gpu_engine_max_percent',
                'gpu_dedicated_bytes', 'gpu_shared_bytes'):
        values = [row[key] for row in rows if row.get(key) is not None]
        result[key] = {'peak': max(values), 'median': percentile(values, .5),
                       'first': values[0], 'last': values[-1]} if values else None
    return result


def response_summary(rows):
    visible = [r['ms'] for r in rows if r['visible']]
    return {'requests': len(rows),
            'visible_ms': {f'p{int(p*100)}': percentile(visible, p) for p in (.5, .95, .99)},
            'timeout_rate': sum(r['timeout'] for r in rows) / len(rows) if rows else None,
            'visible_ratio': len(visible) / len(rows) if rows else None}


def summarize(prefix):
    rows = read(Path(str(prefix) + '.jsonl'))
    prepared = next(row for row in rows if row['event'] == 'prepared')
    requests = [row for row in rows if row['event'] == 'prediction']
    closed = next((row for row in rows if row['event'] == 'closed'), None)
    if not requests or closed is None:
        raise ValueError(f'{prefix}: incomplete run, not a stability pass')
    resources_path = Path(str(prefix) + '.resources.jsonl')
    resources = read(resources_path) if resources_path.exists() else []
    warm = requests[1:]
    visible = [r['ms'] for r in warm if r['visible']]
    result = {'source': str(prefix), 'prepared': prepared,
              'meta': json.loads(Path(str(prefix) + '.meta.json').read_text(encoding='utf-8-sig')),
              'first': requests[0], 'requests': len(requests), 'duration_seconds': closed['seconds'],
              'warm_requests': len(warm),
              'warm_visible_ms': {f'p{int(p*100)}': percentile(visible, p) for p in (.5, .95, .99)},
              'warm_timeout_rate': sum(r['timeout'] for r in warm) / len(warm) if warm else None,
              'warm_visible_ratio': len(visible) / len(warm) if warm else None,
              'results_by_prefix': dict.fromkeys(sorted({r['prefix'] for r in requests})),
              'resource_samples': len(resources), 'resources': resource_summary(resources),
              'warm_by_prefix': {text: response_summary([r for r in warm if r['prefix'] == text])
                                 for text in sorted({r['prefix'] for r in warm})}}
    # Request keys vary by session and must not be treated as quality differences.
    for text in result['results_by_prefix']:
        result['results_by_prefix'][text] = sorted({json.dumps({'visible': r['visible'], 'words': r['words'],
                                                               'error': r['result'].get('Err') if isinstance(r['result'], dict) else 'Timeout'},
                                                              ensure_ascii=False, sort_keys=True)
                                                        for r in requests if r['prefix'] == text})
    if closed['seconds'] >= 600:
        result['warm_time_windows'] = {
            'first_five_minutes': response_summary([r for r in warm if r['elapsed_seconds'] <= 300]),
            'last_five_minutes': response_summary([r for r in warm if r['elapsed_seconds'] >= closed['seconds'] - 300])}
    if result['meta'].get('duration_seconds', 0) > 0 and result['meta'].get('loading') == 'preload_keep':
        # The sampler starts immediately after process creation; prepared.ms starts
        # inside the probe. Exclude a one-second guard at phase edges and label
        # these windows approximate rather than pretending the clocks are shared.
        ready = prepared['ms'] / 1000
        start = ready + 10
        end = start + closed['seconds']
        windows = {'idle': (ready + 1, start - 1),
                   'sustained_first_minute': (start + 1, min(start + 60, end - 1)),
                   'sustained_last_minute': (max(start + 1, end - 60), end - 1)}
        result['resource_windows_approximate'] = {
            label: {'seconds': [begin, finish], 'samples': len(selected),
                    'resources': resource_summary(selected)}
            for label, (begin, finish) in windows.items()
            for selected in [[r for r in resources if begin <= r['seconds'] <= finish]]}
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', nargs='+', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.write_text(json.dumps([summarize(p) for p in args.prefix], ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
