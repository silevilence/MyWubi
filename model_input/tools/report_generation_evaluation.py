"""Paired exact-reference metrics; alternatives require the separate human rubric.

Only authored dev labels choose scoring gates. Generation settings are frozen in
the model-bound policy before holdout inference. All failures remain in the report.
"""
import argparse
import hashlib
import json
from pathlib import Path


def admit(scores, minimum, margin):
    ranked = sorted(scores, key=lambda s: -s['score'])
    if not ranked or ranked[0]['score'] < minimum:
        return []
    if len(ranked) > 1 and ranked[0]['score'] - ranked[1]['score'] < margin:
        return []
    return [row['text'] for row in ranked if row['score'] >= minimum][:3]


def select_gate(samples, runs):
    by_id = {row['id']: row for row in runs}
    dev = [s for s in samples if s['split'] == 'dev']
    if not dev or not any(s['should_stop'] for s in dev):
        raise ValueError('development positives and stop cases required')
    choices = []
    for minimum in (-8., -6., -4., -3., -2., -1.):
        for margin in (0., .5, 1.):
            good = bad = 0
            for sample in dev:
                words = admit(by_id[sample['id']]['scores'], minimum, margin)
                good += bool(set(words) & set(sample['references']))
                bad += sum(word not in sample['references'] for word in words)
            # A transparent experimental utility, not a product accuracy claim.
            choices.append((good - bad, good, minimum, margin))
    _, _, minimum, margin = max(choices)
    return {'minimum_score': minimum, 'minimum_margin': margin,
            'selection': 'maximize dev reference hits minus non-reference displays; tie: hits, stricter gate',
            'grid': {'minimum_score': [-8,-6,-4,-3,-2,-1], 'minimum_margin': [0,.5,1]}}


def dictionary_inventory():
    root = Path(__file__).resolve().parents[2]
    paths = [('core_engine/src/assistance/prediction_words.tsv', 0),
             ('core_engine/src/assistance/prediction_pairs.tsv', 1), ('tables/wubi86.dict', 1)]
    words, hashes = set(), {}
    for name, column in paths:
        path = root / name
        hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        for line in path.read_text(encoding='utf-8').splitlines():
            fields = line.split('\t')
            if not line.startswith('#') and len(fields) > column:
                words.add(fields[column])
    return words, hashes


def summarize(samples, runs, gate, budget, inventory=None):
    by_id = {row['id']: row for row in runs}
    report = {}
    failures = []
    for method in ('dictionary', 'scoring', 'generation'):
        count = positives = negatives = covered = first = topk = false_stop_display = oov = oov_hit = 0
        displayed = exact_displayed = timed_out = recall_hit = dictionary_oov = dictionary_oov_hit = 0
        for sample in samples:
            run = by_id[sample['id']]
            if run['prefix'] != sample['prefix']:
                raise ValueError('mismatched context')
            refs = set(sample['references']); count += 1
            positive = not sample['should_stop']; positives += positive; negatives += not positive
            outside = bool(refs) and not refs.intersection(run['dictionary'])
            oov += outside
            external_refs = refs - inventory if inventory is not None else set()
            dictionary_oov += bool(external_refs)
            words = run['dictionary'][:3] if method == 'dictionary' else (
                admit(run['scores'], gate['minimum_score'], gate['minimum_margin']) if method == 'scoring' else run['generated'][:3])
            recalled = run['dictionary'] if method != 'generation' else run['generated']
            recall_hit += bool(refs.intersection(recalled))
            ms = 0 if method == 'dictionary' else run['score_ms' if method == 'scoring' else 'generation_ms']
            if budget is not None and ms > budget:
                timed_out += 1; words = []
            hit = bool(refs.intersection(words))
            covered += bool(words); first += bool(words and words[0] in refs); topk += hit
            false_stop_display += bool(sample['should_stop'] and words)
            oov_hit += bool(outside and hit)
            dictionary_oov_hit += bool(external_refs.intersection(words))
            displayed += len(words); exact_displayed += sum(w in refs for w in words)
            if (positive and not hit) or (sample['should_stop'] and words):
                failures.append({'method':method,'id':sample['id'],'words':words,'references':sample['references'],
                                 'reason':'timeout' if budget is not None and ms > budget else 'stop-display' if sample['should_stop'] else 'reference-miss'})
        ratio = lambda a,b: a/b if b else None
        report[method] = dict(cases=count, positive_cases=positives, stop_cases=negatives,
            recall=ratio(recall_hit,positives), top1=ratio(first,positives), top3=ratio(topk,positives),
            display_coverage=ratio(covered,count), stop_false_display=ratio(false_stop_display,negatives),
            exact_display_precision=ratio(exact_displayed,displayed), recall_external_cases=oov,
            recall_external_hits=oov_hit, dictionary_external_cases=dictionary_oov,
            dictionary_external_hits=dictionary_oov_hit, timeouts=timed_out)
    return {'metrics':report,'failures':failures}


def chain_summary(runs):
    result = {}
    for method in ('dictionary', 'scoring', 'generation'):
        chains = [c for r in runs for c in r.get('paired_chains', []) if c['method'] == method]
        lengths = [sum(s['selected'] is not None for s in c['steps']) for c in chains]
        result[method] = {'seeds': len(chains), 'selected_steps': sum(lengths),
                          'zero_step_chains': lengths.count(0), 'three_step_chains': lengths.count(3),
                          'lengths': lengths}
    return result


def validate_paired_chains(runs, budget):
    for run in runs:
        chains=run.get('paired_chains',[])
        if sorted(c['method'] for c in chains) != ['dictionary','generation','scoring']:
            raise ValueError('every seed requires all three methods, including stopped chains')
        for chain in chains:
            prefix=run['prefix'];stopped=False
            if not 1<=len(chain['steps'])<=3:
                raise ValueError('invalid chain length')
            for step in chain['steps']:
                if stopped or step['prefix']!=prefix:
                    raise ValueError('chain lost the acknowledged prefix')
                word=step['selected']
                if word is None:
                    stopped=True
                else:
                    if not step['choices'] or word!=step['choices'][0] or step['ms']>budget:
                        raise ValueError('chain selected an unavailable or late candidate')
                    prefix+=word
            if prefix!=chain['final_prefix']:
                raise ValueError('chain final text differs from its append steps')


def blind_review(runs, gate):
    """Keep method names, references and scores out of the reviewer sheet."""
    sheet, key = [], []
    for run in runs:
        methods = {'dictionary': run['dictionary'][:3],
                   'scoring': admit(run['scores'], gate['minimum_score'], gate['minimum_margin']),
                   'generation': run['generated'][:3]}
        entries = [(method, 'candidate', run['prefix'], words) for method, words in methods.items()]
        entries += [(c['method'], 'chain', run['prefix'], [s['selected'] for s in c['steps'] if s['selected'] is not None])
                    for c in run.get('paired_chains', [])]
        for method, kind, prefix, words in entries:
            opaque = hashlib.sha256(f"mi26-acceptance|{run['id']}|{method}|{kind}".encode()).hexdigest()[:16]
            sheet.append({'review_id': opaque, 'kind': kind, 'prefix': prefix, 'output': words,
                          'reviewer_id': '', 'reviewer_kind': '', 'judgment': '', 'reason': ''})
            key.append({'review_id': opaque, 'sample_id': run['id'], 'method': method, 'kind': kind})
    return sorted(sheet, key=lambda r: r['review_id']), key


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('dev',type=Path);parser.add_argument('dev_runs',type=Path)
    parser.add_argument('holdout',type=Path);parser.add_argument('holdout_runs',type=Path)
    parser.add_argument('output',type=Path)
    parser.add_argument('--plan',type=Path)
    parser.add_argument('--review-sheet',type=Path)
    args=parser.parse_args()
    dev=json.loads(args.dev.read_text(encoding='utf-8'))['samples']
    holdout=json.loads(args.holdout.read_text(encoding='utf-8'))['samples']
    dev_runs=json.loads(args.dev_runs.read_text(encoding='utf-8-sig'))
    test_runs=json.loads(args.holdout_runs.read_text(encoding='utf-8-sig'))
    for samples, result in ((dev,dev_runs),(holdout,test_runs)):
        expected=[s['id'] for s in samples];actual=[r['id'] for r in result['runs']]
        if len(set(expected))!=len(expected) or len(set(actual))!=len(actual) or set(expected)!=set(actual):
            raise ValueError('missing, duplicate or extra sample results')
    if dev_runs['policy'] != test_runs['policy'] or dev_runs['model_sha256'] != test_runs['model_sha256']:
        raise ValueError('model/policy changed between dev and holdout')
    # Empty context is an intentionally shared contract case, not quality data.
    if {s['prefix'] for s in dev if s['prefix']} & {s['prefix'] for s in holdout if s['prefix']}:
        raise ValueError('development/holdout context leakage')
    gate=select_gate(dev,dev_runs['runs'])
    if args.plan:
        plan=json.loads(args.plan.read_text(encoding='utf-8'))
        if plan['dataset_sha256'] != hashlib.sha256(args.holdout.read_bytes()).hexdigest():
            raise ValueError('holdout differs from frozen plan')
        if any(plan['scoring_gate'][k] != gate[k] for k in ('minimum_score','minimum_margin')):
            raise ValueError('development rerun differs from frozen scoring gate')
        if plan['model_sha256'] != test_runs['model_sha256']:
            raise ValueError('model differs from frozen plan')
        if plan['policy_sha256'] != test_runs.get('policy_sha256') or plan['dataset_sha256'] != test_runs.get('dataset_sha256'):
            raise ValueError('inference run differs from frozen policy/dataset')
        gate=plan['scoring_gate']
        validate_paired_chains(test_runs['runs'],plan['budget_ms'])
    inventory, dictionary_hashes = dictionary_inventory()
    result={'source_hashes':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in (args.dev,args.dev_runs,args.holdout,args.holdout_runs)},
        'scoring_gate':gate,'generation_policy':test_runs['policy'],
        'dictionary_inventory': {'hashes':dictionary_hashes,'user_dictionary':'empty',
            'definition':'Exact candidate string absent from all listed dictionaries; word remainders counted as strings.'},
        'quality':summarize(holdout,test_runs['runs'],gate,None,inventory),
        'budget_1000ms':summarize(holdout,test_runs['runs'],gate,1000,inventory),
        'chains':[r for r in test_runs['runs'] if r['chain']],
        'paired_chain_summary':chain_summary(test_runs['runs']),
        'paired_chains':[{'id':r['id'],'chains':r.get('paired_chains',[])} for r in test_runs['runs']],
        'external_edits':[{'id':r['id'],'isolated':r.get('edit_isolated')} for r in test_runs['runs'] if r.get('edited_from') is not None],
        'limitations':['Single-reference matches are not semantic correctness; reasonable alternatives remain unjudged.',
            'Recall-external means absent from this prefix recall, not proven absent from every dictionary.',
            'Wiki source is independent of authored development prompts, not guaranteed absent from model pretraining.',
            'No independent human annotation of alternatives or chains; no recommendation or accuracy improvement claim.']}
    if args.review_sheet:
        sheet,key=blind_review(test_runs['runs'],gate)
        args.review_sheet.write_text(json.dumps(sheet,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        args.review_sheet.with_suffix('.key.json').write_text(json.dumps(key,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    args.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(result['quality']['metrics'],ensure_ascii=False,indent=2))


if __name__=='__main__':main()
