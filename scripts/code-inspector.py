#!/usr/bin/env python3
"""Run the code inspector unattended and verify its controlled world independently."""
import argparse
import json
from pathlib import Path
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha


def validate(stdout):
    rows = [json.loads(line) for line in stdout.splitlines()]
    assert len(rows) == 4, 'missing/extra output'
    for row, phase, direct, answer, undocumented in zip(rows[:3],
            ['initial', 'replaced', 'removed'],
            [['answer'], ['answer', 'extra'], ['extra']],
            [20, 42, 10], [[], ['extra'], ['extra']]):
        assert row['phase'] == phase
        assert row['direct'] == direct
        assert row['answer'] == answer
        assert row['aliases'] == ['InspectorChild', 'InspectorAlias']
        assert row['undocumented'] == undocumented
        assert row['inspection_fact_delta'] == 0
        assert isinstance(row['inspection_cpu_ns'], int) and row['inspection_cpu_ns'] > 0
    assert rows[3] == {'history': [20, 42, 10], 'passed': True}
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('prefix', type=Path)
    p.add_argument('--samples', type=int, default=3)
    args = p.parse_args()
    if args.samples < 1: p.error('samples must be positive')
    prefix = args.prefix.resolve()
    work = Path(tempfile.mkdtemp(prefix='birnam-code-inspector-'))
    source = ROOT/'examples/code-inspector/run.birnam'
    runtime_files = [prefix/'bin/birnam', *sorted((prefix/'lib').glob('*.dylib')),
                     *sorted((prefix/'lib').glob('*.image'))]
    runtime = {str(f.relative_to(prefix)): sha(f) for f in runtime_files}
    receipt = {'prefix': str(prefix),
               'source_sha256': {str(f.relative_to(ROOT)): sha(f) for f in [source, Path(__file__).resolve()]},
               'runtime_sha256': runtime, 'runs': [], 'passed': False}
    for i in range(args.samples):
        run = command([prefix/'bin/birnam', source], cwd=ROOT, env=clean_environment(), timeout=180)
        (work/f'run-{i}.log').write_text(run['output'])
        receipt['runs'].append(run)
        save_json(work/'results.json', receipt)
        if run['exit_code'] or run['stderr']: raise SystemExit(f'inspector failed; {work}')
        try:
            run['rows'] = validate(run['stdout'])
        except (AssertionError, KeyError, ValueError, TypeError) as error:
            raise SystemExit(f'inspector output rejected ({error}); {work}/results.json') from error
    assert runtime == {str(f.relative_to(prefix)): sha(f) for f in runtime_files}, 'runtime changed during experiment'
    receipt['passed'] = True
    save_json(work/'results.json', receipt)
    print(f'code-inspector: {args.samples} processes passed; {work}/results.json')


if __name__ == '__main__': main()
