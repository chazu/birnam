#!/usr/bin/env python3
"""Explain the current Block constructor protocol in a complete Id send trace."""
import argparse
import collections
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('trace', type=Path)
    parser.add_argument('accounting', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    data = args.trace.read_bytes()
    receipt = json.loads(data)
    accounting = json.loads(args.accounting.read_text())
    assert hashlib.sha256(data).hexdigest() == accounting['input_sha256']
    measured = receipt['raw']['cases'][0]['work_counts']['candidate']['steady_messages']
    trace = measured['trace']
    operations = measured['operations']
    by_id = {row['id']: row for row in trace}
    children = collections.defaultdict(list)
    for row in trace:
        children[row['parent']].append(row)
    totals = collections.Counter()
    groups = accounting['closure_groups']
    cuts = {row['id']: row for row in accounting['attribution_cut']}
    for group in groups:
        captures = group['capture_count']
        constructor = by_id[group['ctor_id']]
        stages = collections.Counter({'entry/admission': 1})  # Constructor send.
        stage = 'program shape export'
        for child in children[constructor['id']]:
            if child['selector'] == 'prepareAllocation':
                stages['entry/admission'] += child['inclusive']
                continue
            if child['selector'] == 'intern:' and child['key'] == 'imageSizeOf:':
                stage = 'capture storage validation'
            elif child['selector'] == 'intern:' and child['key'] == 'slotAt:in:':
                stage = 'capture reread/copy'
            elif child['selector'] == 'delegated':
                stage = 'Block behavior/method/allocation'
            stages[stage] += child['inclusive']
        assert sum(stages.values()) == constructor['inclusive']
        assert dict(stages) == {key: value for key, value in {
            'entry/admission': 2,
            'program shape export': 84,
            'capture storage validation': 12,
            'capture reread/copy': 7 * captures,
            'Block behavior/method/allocation': 12,
        }.items() if value}, (constructor['id'], captures, dict(stages))
        assert group['sends'] == 168 + 23 * captures
        totals.update(stages)
        for cut_id in group['cut_ids']:
            cut = cuts[cut_id]
            if cut_id == constructor['id']:
                continue
            stage = {
                'allocateReferenceSlots:': 'temporary capture storage',
                'slotAt:in:put:': 'temporary capture writes',
                'at:': 'generated factory lookup',
                'retain:': 'generated returned-object roots',
            }[cut['selector']]
            totals[stage] += cut['sends']
    totals['capture access'] = next(row['count'] for row in accounting['partition']
                                   if row['category'] == 'capture access')
    expected = sum(row['count'] for row in accounting['partition']
                   if row['category'] in ('closure construction', 'capture access'))
    assert sum(totals.values()) == expected
    first_plus = next(row for row in trace if row['selector'] == '+')
    projections = collections.Counter(row['parent'] for row in trace
        if first_plus['id'] <= row['id'] < first_plus['id'] + first_plus['inclusive']
        and row['selector'] == 'captureWord:of:')
    examples = []
    for parent, count in projections.items():
        row = by_id[parent]
        ancestors = []
        cursor = row
        while cursor['parent']:
            cursor = by_id[cursor['parent']]
            ancestors.append(cursor['selector'])
        examples.append({'activation_id': parent, 'selector': row['selector'],
                         'capture_projections': count, 'ancestors': ancestors})
    assert sum(projections.values()) == 69
    result = {
        'schema': 1,
        'scope': accounting['scope'],
        'trace_sha256': accounting['input_sha256'],
        'source_revision': receipt['source_revision'],
        'source_sha256': receipt['source_sha256'],
        'operations': operations,
        'closure_sends_per_addition': expected / operations,
        'construction_formula': '168 + 23 * captures',
        'constructors_checked': len(groups),
        'purpose_partition': [{'purpose': purpose, 'sends_per_addition': count / operations}
                              for purpose, count in totals.items()],
        'first_addition_capture_activations': examples,
        'checks': ['raw trace hash matches accounting', 'each constructor subtree fully partitioned',
                   'fixed and per-capture costs match every constructor',
                   'all closure construction/access sends accounted for'],
        'limits': 'Source-dependent classification of current protocol. No CPU estimate, '
                  'elision proof or optimal send-count bound.',
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    for row in result['purpose_partition']:
        print(row['purpose'], row['sends_per_addition'])


if __name__ == '__main__':
    main()
