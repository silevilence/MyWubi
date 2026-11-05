import unittest
from report_generation_evaluation import select_gate, summarize, chain_summary, blind_review, validate_paired_chains

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

    def test_blind_review_keeps_empty_chains_without_revealing_method(self):
        run=dict(id='test',prefix='结束。',dictionary=[],scores=[],generated=[],
                 paired_chains=[dict(method=m,steps=[dict(selected=None)],final_prefix='结束。')
                                for m in ('dictionary','scoring','generation')])
        summary=chain_summary([run])
        self.assertEqual(summary['generation']['zero_step_chains'],1)
        sheet,key=blind_review([run],dict(minimum_score=-6,minimum_margin=1))
        self.assertEqual(len(sheet),6)
        self.assertEqual(len({r['review_id'] for r in sheet}),6)
        self.assertTrue(all('method' not in r and 'references' not in r for r in sheet))
        self.assertEqual({r['method'] for r in key},{'dictionary','scoring','generation'})

    def test_chain_cannot_count_late_or_unacknowledged_selection(self):
        chains=[dict(method=m,steps=[dict(prefix='北京',choices=['市'],selected='市',ms=10)],final_prefix='北京市')
                for m in ('dictionary','scoring','generation')]
        runs=[dict(prefix='北京',paired_chains=chains)]
        validate_paired_chains(runs,1000)
        chains[2]['steps'][0]['ms']=1001
        with self.assertRaises(ValueError):validate_paired_chains(runs,1000)
        chains[2]['steps'][0]['ms']=10
        chains[2]['final_prefix']='北京大学'
        with self.assertRaises(ValueError):validate_paired_chains(runs,1000)

if __name__=='__main__':unittest.main()
