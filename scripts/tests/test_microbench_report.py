import sys
from pathlib import Path
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from microbench_report import summarize


def case(ratios):
    return {'case': 'fixture', 'work_counts': {}, 'samples': [
        {'batches': [
            {'variant': 'baseline', 'iterations': 100, 'cpu_ns': 10_000_000, 'wall_ns': 20_000_000},
            {'variant': 'candidate', 'iterations': 50, 'cpu_ns': int(5_000_000*r), 'wall_ns': 20_000_000}
        ]} for r in ratios]}


class ReportTests(unittest.TestCase):
    def test_normalizes_operations_and_excludes_scheduling_wait(self):
        result = summarize(case([.8]*12))
        self.assertEqual(result['verdict'], 'faster')
        self.assertAlmostEqual(result['candidate_over_baseline'], .8)
        self.assertEqual(result['pairs_with_scheduling_delay'], 12)

    def test_public_operation_normalization(self):
        fixture = case([.8]*12)
        fixture['operations_per_invocation'] = 16
        result = summarize(fixture)
        self.assertEqual(result['median_baseline_cpu_ns_per_language_operation'], 6250)
        self.assertEqual(result['median_candidate_cpu_ns_per_language_operation'], 5000)

    def test_short_and_noisy_runs_are_inconclusive(self):
        self.assertEqual(summarize(case([.8]*3))['verdict'], 'inconclusive')
        self.assertEqual(summarize(case([.5,1.5]*10))['verdict'], 'inconclusive')

    def test_slower_and_equivalent_results(self):
        self.assertEqual(summarize(case([1.2]*12))['verdict'], 'slower')
        self.assertEqual(summarize(case([1.0]*12))['verdict'], 'no meaningful difference')


if __name__ == '__main__': unittest.main()
