#!/usr/bin/env python3
"""Unattended CPU phase measurements with an independent JSON result oracle.

No timing thresholds. Native sampling uses the profile project's `sample` mode
and its ready/stdin handshake; sampled runs must remain separate from timings.
"""
import argparse
import json
import shutil
from pathlib import Path
from validation_support import ROOT, clean_environment, command, save_json, sha
from log_summary_data import generate


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--iterations', type=int, default=3)
    args = parser.parse_args()
    if not 1 <= args.iterations <= 10:
        parser.error('--iterations must be 1..10')
    work = args.work.resolve()
    if work == ROOT or ROOT.is_relative_to(work):
        parser.error('work must be a dedicated directory')
    marker = work/'log-profile-work.json'
    if work.exists() and not marker.exists():
        parser.error('work directory is not owned by this runner')
    work.mkdir(parents=True, exist_ok=True)
    save_json(marker, {'purpose': 'disposable log-summary CPU profiling'})
    shutil.copytree(ROOT/'examples/log-summary', work/'examples/log-summary', dirs_exist_ok=True,
                    ignore=shutil.ignore_patterns('.birnam'))
    shutil.copytree(ROOT/'modules', work/'modules', dirs_exist_ok=True,
                    ignore=shutil.ignore_patterns('.birnam'))
    manifest = generate(work/'data')
    fixture = work/'data/small.jsonl'
    lines = [line.strip() for line in fixture.read_text().splitlines() if line.strip()]
    records = list(map(json.loads, lines))
    expected = json.loads((work/'data/small.expected.json').read_text())
    prefix = args.prefix.resolve(); binary = prefix/'bin/birnam'
    project = work/'examples/log-summary/profile'
    env = clean_environment()
    built = command([binary, 'build', project], cwd=work, env=env, timeout=120)
    if built['exit_code']:
        raise RuntimeError(built)
    result = {'prefix': str(prefix), 'fixture_sha256': sha(fixture), 'manifest': manifest,
              'source_sha256': {str(p.relative_to(ROOT)): sha(p) for p in [
                  ROOT/'examples/log-summary/profile/main.birnam', ROOT/'modules/json.birnam',
                  ROOT/'examples/log-summary/src/log-summary.birnam', Path(__file__)]},
              'runtime_sha256': {p.name: sha(p) for p in sorted((prefix/'lib').glob('*.dylib'))},
              'qualification': 'Independent warmed phases with prepared inputs retained; not additive attribution. Timing mode retains normal GC. Allocation mode disables automatic GC for one measured pass; its time is not the timing baseline.',
              'runs': {}}
    for mode, count in [('time', args.iterations), ('allocation', 1)]:
        run = command([binary, 'run', project, '--', fixture, 'all', count, mode],
                      cwd=work, env=env, timeout=120)
        if run['exit_code'] or run['stderr']:
            raise RuntimeError(run)
        data = json.loads(run['stdout'])
        assert data['report'] == expected and json.loads(data['rendered']) == expected
        string_values = list(map(json.loads, data['string_inputs']))
        number_values = list(map(json.loads, data['number_inputs']))
        checks = {'empty': None, 'full': data['rendered'], 'split_trim': lines,
                  'parse': records, 'peg': [len(line.encode()) for line in lines],
                  'lower': records, 'validate': [True]*len(lines), 'render': data['rendered'],
                  'decode_strings': string_values, 'unescape': string_values,
                  'numbers': number_values}
        for row in data['results']:
            assert json.loads(row['check_json']) == checks[row['phase']], row['phase']
            assert len(row['samples']) == count
        result['runs'][mode] = data
        save_json(work/'results.json', result)
        print(mode, 'all phase results matched the independent oracle', flush=True)
    print(work/'results.json')


if __name__ == '__main__':
    main()
