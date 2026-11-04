"""Authored tests of calibration mechanics; these are not model quality data."""
import copy
import unittest
from calibrate_prediction import calibrate, validate


def fixture():
    samples = []
    for split in ('dev', 'test'):
        for i, (acceptable, scores) in enumerate([(['天气'], [-1, -4]), (['预报'], [-4, -1]), ([], [-8, -9])]):
            samples.append(dict(id=f'{split}-{i}', group=f'{split}-{i}', split=split,
                                prefix=f'{split}-{i}', acceptable=acceptable,
                                candidates=[dict(text=t, score=s) for t, s in zip(['天气', '预报'], scores)]))
    return dict(adapter='gguf-continuation-v1', model_sha256='a' * 64, vocabulary_sha256=None, samples=samples)


class CalibrationTest(unittest.TestCase):
    def test_abstains_on_negatives_and_filters_weak_items(self):
        r = calibrate(fixture(), 1.0, 0.5)
        self.assertEqual(r['dev']['shown'], 2)
        self.assertEqual(r['test']['false_show'], 0)
        self.assertEqual(r['test']['candidate_precision'], 1.0)

    def test_test_labels_cannot_select_thresholds(self):
        data = fixture()
        expected = calibrate(data)['selected']
        for s in data['samples']:
            if s['split'] == 'test':
                s['acceptable'] = ['different']
        self.assertEqual(calibrate(data)['selected'], expected)

    def test_leakage_nonfinite_and_missing_negatives_are_rejected(self):
        for kind in ('leak', 'score', 'negative'):
            data = copy.deepcopy(fixture())
            if kind == 'leak': data['samples'][-1]['prefix'] = data['samples'][0]['prefix']
            if kind == 'score': data['samples'][0]['candidates'][0]['score'] = float('nan')
            if kind == 'negative': data['samples'][2]['acceptable'] = ['天气']
            with self.assertRaises(ValueError): validate(data)

    def test_no_qualifying_threshold_does_not_emit_a_policy(self):
        data = fixture()
        for s in data['samples']:
            if s['acceptable']: s['acceptable'] = ['missing']
        self.assertIsNone(calibrate(data)['selected'])


if __name__ == '__main__':
    unittest.main()
