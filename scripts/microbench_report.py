"""Summarize paired CPU samples without treating a busy host as a reason to wait."""
import math
import random
import statistics


def summarize(case):
    ratios, baseline, candidate, waits = [], [], [], 0
    for pair in case['samples']:
        batches = {b['variant']: b for b in pair['batches']}
        if set(batches) != {'baseline', 'candidate'}:
            continue
        a, b = batches['baseline'], batches['candidate']
        if min(a['cpu_ns'], b['cpu_ns']) < 1_000_000:
            continue  # calibration/timer overhead dominates short samples
        baseline.append(a['cpu_ns'] / a['iterations'])
        candidate.append(b['cpu_ns'] / b['iterations'])
        ratios.append(math.log(candidate[-1] / baseline[-1]))
        waits += any(x['cpu_ns'] / max(1, x['wall_ns']) < .8 for x in (a, b))
    if not ratios:
        return {'case': case['case'], 'verdict': 'inconclusive', 'pairs': 0}
    rng = random.Random(0)
    draws = sorted(statistics.median(rng.choices(ratios, k=len(ratios))) for _ in range(1000))
    low, high = math.exp(draws[24]), math.exp(draws[974])
    ratio = math.exp(statistics.median(ratios))
    verdict = 'inconclusive'
    if len(ratios) >= 8 and high - low <= .15:
        if high < .97:
            verdict = 'faster'
        elif low > 1.03:
            verdict = 'slower'
        elif low >= .97 and high <= 1.03:
            verdict = 'no meaningful difference'
    operations = case.get('operations_per_invocation', 1)
    return {'case': case['case'], 'verdict': verdict, 'pairs': len(ratios),
            'operations_per_invocation': operations,
            'median_baseline_cpu_ns_per_language_operation': statistics.median(baseline) / operations,
            'median_candidate_cpu_ns_per_language_operation': statistics.median(candidate) / operations,
            'candidate_over_baseline': ratio, 'bootstrap_interval_95': [low, high],
            'median_baseline_cpu_ns_per_op': statistics.median(baseline),
            'median_candidate_cpu_ns_per_op': statistics.median(candidate),
            'pairs_with_scheduling_delay': waits,
            'allocation_counts': case['work_counts'],
            'interval_note': 'Within-run uncertainty only; does not cover drift, thermal changes or systematic bias.'}
