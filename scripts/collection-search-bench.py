#!/usr/bin/env python3
"""Compare installed shared detect: drivers with identical authored workloads (macOS)."""
import argparse
import json
import platform
import statistics
from pathlib import Path
from validation_support import ROOT, clean_environment, command, save_json, sha


def source(samples):
    cases = []
    for family, size in [('vector', 8), ('vector', 128), ('vector', 1024),
                         ('range', 128), ('string', 128), ('map', 128)]:
        for position, target in [('first', 1), ('middle', size // 2), ('last', size), ('absent', 0)]:
            if family == 'string':
                text = 'a' * size if not target else 'a' * (target - 1) + 'z' + 'a' * (size - target)
                collection = json.dumps(text)
                needle = '"z"'
                expected = '"z"' if target else 'nil'
            else:
                collection = f'[1 to: {size}]'
                if family == 'vector': collection = f'[{collection} asVector]'
                if family == 'map':
                    # Use values order to choose the actual first/middle/last HAMT entry.
                    collection = f'[{collection} inject: {{}} into: [:map :i | [map at: i put: i]]]'
                needle = str(target)
                expected = str(target) if target else 'nil'
                if family == 'map' and target:
                    needle = f'[[values values] at: {target - 1}]'
                    expected = 'needle'
            repetitions = max(4, 2048 // size)
            label = f'{family}-{size}-{position}'
            cases.append(f'''[| let values := {collection}; let needle := {needle};
  let expected := {expected};
  let operation := [| [{repetitions} timesRepeat: [|
    let answer := [values detect: [:each | [each = needle]]];
    [[answer = expected] ifTrue: [| nil] ifFalse: [| [Error kind: #searchMismatch]]]]]];
  let warm := [operation value];
  [[warm isError] ifTrue: [| [System println: warm]; [OS exit: 1]]];
  {{"case" -> "{label}" "repetitions" -> {repetitions}
    "samples" -> [[1 to: {samples}] collect: [:sample |
      [System gc]; let before := [16 clockNanos];
      let result := [operation value]; let used := [[16 clockNanos] - before];
      [[result isError] ifTrue: [| [System println: result]; [OS exit: 1]]]; used]]}}
]''')
    return '[Module load: #time]; [Module load: #json]; [Module load: #os];\n' + \
        '[System println: [Json stringify: [#[' + '\n'.join(cases) + '] collect: [:case | [case value]]]]];\n'


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--baseline', required=True, type=Path)
    p.add_argument('--candidate', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    p.add_argument('--samples', type=int, default=3)
    p.add_argument('--order', default='baseline,candidate,candidate,baseline')
    a = p.parse_args()
    if platform.system() != 'Darwin': p.error('clock ID 16 is macOS process CPU time')
    if not 1 <= a.samples <= 10: p.error('samples must be 1..10')
    order = a.order.split(',')
    if any(x not in ('baseline', 'candidate') for x in order): p.error('invalid order')
    a.output.parent.mkdir(parents=True, exist_ok=True)
    script = a.output.with_suffix('.birnam')
    script.write_text(source(a.samples))
    prefixes = {key: getattr(a, key).resolve() for key in ('baseline', 'candidate')}
    report = {'scope': 'shared detect:, normal GC, warmed, setup outside timers; per-search CPU includes result check and repetition driver',
              'platform': platform.platform(), 'source': script.read_text(), 'source_sha256': sha(script),
              'driver_sha256': sha(Path(__file__)), 'prefixes': {k: str(v) for k, v in prefixes.items()},
              'runtime_sha256': {k: {str(f.relative_to(v)): sha(f) for f in
                  [v/'bin/birnam', *sorted((v/'lib').glob('*.dylib')), *sorted((v/'lib').glob('*.image'))]}
                  for k, v in prefixes.items()}, 'runs': []}
    for variant in order:
        run = command([prefixes[variant]/'bin/birnam', script.resolve()], cwd=ROOT,
                      env=clean_environment(), timeout=300)
        if run['exit_code'] or run['stderr']: raise RuntimeError(run)
        rows = json.loads(run['stdout'])
        if len(rows) != 24 or len({r['case'] for r in rows}) != 24: raise ValueError('missing cases')
        for row in rows:
            if len(row['samples']) != a.samples or any(n <= 0 for n in row['samples']):
                raise ValueError('invalid samples')
        report['runs'].append({'variant': variant, 'rows': rows})
        save_json(a.output, report)
        print(variant, '24 checked cases', flush=True)
    report['summary'] = {}
    for label in [r['case'] for r in rows]:
        medians = {}
        for variant in prefixes:
            values = [n/r['repetitions'] for run in report['runs'] if run['variant'] == variant
                      for r in run['rows'] if r['case'] == label for n in r['samples']]
            if values: medians[variant] = statistics.median(values)
        if len(medians) == 2:
            medians['cpu_reduction_percent'] = 100*(1-medians['candidate']/medians['baseline'])
        report['summary'][label] = medians
    save_json(a.output, report)
    print(json.dumps(report['summary'], indent=2))


if __name__ == '__main__': main()
