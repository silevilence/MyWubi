import json
import tempfile
import unittest
from pathlib import Path
from report_device_comparison import percentile, resource_summary, summarize


class ReportTests(unittest.TestCase):
    def test_nearest_rank_and_missing_measurement(self):
        self.assertEqual(percentile(list(range(1, 21)), .95), 19)
        self.assertEqual(percentile(list(range(1, 21)), .99), 20)
        self.assertIsNone(percentile([], .95))
        self.assertIsNone(resource_summary([{'working_set': 20}])['gpu_shared_bytes'])
        self.assertEqual(resource_summary([{'working_set': 20}, {'working_set': 40}])['working_set']['peak'], 40)

    def test_timeout_is_counted_and_incomplete_run_cannot_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            prefix = Path(directory) / 'sample'
            rows = [{'event': 'prepared'},
                    {'event': 'prediction', 'ms': 10, 'visible': True, 'timeout': False,
                     'words': ['市'], 'prefix': '北京', 'result': {'Ok': {}}},
                    {'event': 'prediction', 'ms': 1001, 'visible': False, 'timeout': True,
                     'words': [], 'prefix': '北京', 'result': None}]
            data = Path(str(prefix) + '.jsonl')
            data.write_text('\n'.join(json.dumps(r) for r in rows), encoding='utf-8')
            Path(str(prefix) + '.meta.json').write_text('{}', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'incomplete run'):
                summarize(prefix)
            rows.append({'event': 'closed', 'seconds': 1.1})
            data.write_text('\n'.join(json.dumps(r) for r in rows), encoding='utf-8')
            result = summarize(prefix)
            self.assertEqual(result['warm_timeout_rate'], 1)
            self.assertEqual(result['warm_visible_ratio'], 0)
            self.assertEqual(result['warm_by_prefix']['北京']['timeout_rate'], 1)
            self.assertIsNone(result['warm_visible_ms']['p95'])
            self.assertIsNone(result['resources']['gpu_shared_bytes'])


if __name__ == '__main__':
    unittest.main()
