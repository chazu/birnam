#!/usr/bin/env python3
"""Compare two authored JSON decoders on one runtime, with independent results."""
import argparse
import json
import shutil
import statistics
from pathlib import Path
from log_summary_data import generate
from validation_support import ROOT, clean_environment, command, save_json, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--baseline-json', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    args = parser.parse_args()
    work = args.work.resolve()
    if work == ROOT or ROOT.is_relative_to(work):
        parser.error('work must be a dedicated directory')
    marker = work/'json-bench-work.json'
    if work.exists() and not marker.exists():
        parser.error('work directory is not owned by this runner')
    work.mkdir(parents=True, exist_ok=True)
    save_json(marker, {'purpose': 'disposable JSON decoder comparison'})
    prefix = args.prefix.resolve()
    binary = prefix/'bin/birnam'
    sources = {'baseline': args.baseline_json.resolve(), 'candidate': ROOT/'modules/json.birnam'}
    before = {k: sha(v) for k, v in sources.items()}
    env = clean_environment()
    manifest = generate(work/'data')
    fixture = work/'data/small.jsonl'
    expected = json.loads((work/'data/small.expected.json').read_text())
    projects = {}
    for label, source in sources.items():
        root = work/label
        shutil.copytree(ROOT/'examples/log-summary', root/'examples/log-summary', dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns('.birnam'))
        shutil.copytree(ROOT/'modules', root/'modules', dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns('.birnam'))
        shutil.copy2(source, root/'modules/json.birnam')
        project = root/'examples/log-summary/profile'
        projects[label] = project
        # An isolated workload of short escaped bodies makes scan overhead visible.
        p = project/'main.birnam'
        code = p.read_text().replace('  "empty" -> [| nil]',
            '  "escaped_short" -> [| [[1 to: 250] collect: [:i | ["\\\\n" jsonUnescape]]]]\n  "empty" -> [| nil]')
        p.write_text(code)
        built = command([binary, 'build', project], cwd=work, env=env, timeout=120)
        if built['exit_code']:
            raise RuntimeError(built)
    evidence = {'schema': 1, 'source_sha256': before, 'fixture': manifest['valid']['small'],
                'runtime_sha256': {p.name: sha(p) for p in [binary, *sorted((prefix/'lib').glob('*.dylib'))]},
                'runner_sha256': sha(Path(__file__)), 'profile_sha256': sha(ROOT/'examples/log-summary/profile/main.birnam'),
                'method': 'ABBA per phase, one warmup and 3 timed iterations per process. Same runtime; normal GC. Independent workloads, not additive attribution.',
                'rows': []}
    for phase in ['full', 'unescape', 'escaped_short']:
        for label in ['baseline', 'candidate', 'candidate', 'baseline']:
            result = command([binary, 'run', projects[label], '--', fixture, phase, 3, 'time'],
                             cwd=work, env=env, timeout=120)
            if result['exit_code'] or result['stderr']:
                raise RuntimeError(result)
            data = json.loads(result['stdout'])
            assert data['report'] == expected and json.loads(data['rendered']) == expected
            row = data['results'][0]
            observed = json.loads(row['check_json'])
            if phase == 'full':
                assert json.loads(observed) == expected
            elif phase == 'unescape':
                assert observed == list(map(json.loads, data['string_inputs']))
            else:
                assert observed == ['\n']*250
            evidence['rows'].append({'variant': label, 'phase': phase, 'samples': row['samples']})
            save_json(work/'results.json', evidence)
            print(phase, label, round(statistics.median(s['cpu_ns'] for s in row['samples'])/1e6, 3), flush=True)
    evidence['medians'] = {}
    for phase in ['full', 'unescape', 'escaped_short']:
        med = {label: statistics.median(s['cpu_ns'] for r in evidence['rows']
                    if r['variant'] == label and r['phase'] == phase for s in r['samples'])
               for label in sources}
        evidence['medians'][phase] = {'cpu_ns': med, 'speedup': med['baseline']/med['candidate']}
    assert before == {k: sha(v) for k, v in sources.items()}, 'decoder sources changed during comparison'
    evidence['status'] = 'passed'
    save_json(work/'results.json', evidence)
    print(work/'results.json')


if __name__ == '__main__':
    main()
