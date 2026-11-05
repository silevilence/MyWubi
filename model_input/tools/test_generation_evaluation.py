import unittest
from report_generation_evaluation import select_gate, summarize

class EvaluationTest(unittest.TestCase):
    def test_holdout_labels_do_not_choose_parameters_and_timeouts_are_not_visible(self):
        samples=[dict(id='dev',split='dev',prefix='北',references=['京'],should_stop=False),
                 dict(id='stop',split='dev',prefix='结束。',references=[],should_stop=True),
                 dict(id='test',split='holdout',prefix='量子',references=['计算'],should_stop=False)]
        runs=[dict(id=s['id'],prefix=s['prefix'],dictionary=['京'],scores=[dict(text='京',score=-2)],score_ms=5,generated=['计算'],generation_ms=1500) for s in samples]
        gate=select_gate(samples,runs)
        samples[2]['references']=['完全不同']
        self.assertEqual(gate,select_gate(samples,runs))
        metrics=summarize(samples[2:],runs,gate,1000)['metrics']['generation']
        self.assertEqual(metrics['display_coverage'],0)
        self.assertEqual(metrics['timeouts'],1)

if __name__=='__main__':unittest.main()
