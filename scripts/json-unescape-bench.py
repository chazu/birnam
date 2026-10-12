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
    parser.add_argument('--phases', default='full,unescape,escaped_short',
                        help='comma-separated workload names; see the allowed set below')
    args = parser.parse_args()
    phases = args.phases.split(',')
    allowed = {'full', 'unescape', 'decode_strings', 'escaped_short', 'quoted_escaped_short', 'numbers', 'float_numbers', 'long_float_numbers', 'long_integer_numbers', 'parse_floats', 'parse_long_floats'}
    if len(set(phases)) != len(phases) or any(phase not in allowed for phase in phases):
        parser.error('phases must be distinct supported names')
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
    numeric_texts = {
        'float': ['1.5', '-2.25', '1e3', '1E-3', '0.125', '-0.0'] * 25,
        'long_float': ['1.234567890123456789012345', '-0.000000000000000001',
                       '1234567890123456789.0', '1234567890123456789e-5',
                       '1234567890123456789E-5'] * 30,
        'long_integer': ['9223372036854775807', '-9223372036854775808',
                         '9223372036854775808', '-9223372036854775809',
                         '1000000000000000000'] * 30,
    }
    numeric_expected = {k: list(map(json.loads, values)) for k, values in numeric_texts.items()}
    numeric_expected['long_integer'] = [v if -(1 << 63) <= v < (1 << 63) else None
                                        for v in numeric_expected['long_integer']]
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
        quoted = json.dumps(json.dumps('\n'))
        extra = ('  "escaped_short" -> [| [[1 to: 250] collect: [:i | ["\\\\n" jsonUnescape]]]]\n'
                 f'  "quoted_escaped_short" -> [| [[1 to: 250] collect: [:i | [{quoted} jsonDecodeString]]]]\n')
        code = p.read_text().replace('  "empty" -> [| nil]', extra + '  "empty" -> [| nil]')
        numeric_setup = ''
        numeric_cases = ''
        for kind, values in numeric_texts.items():
            text = '[' + ','.join(values) + ']'
            numeric_setup += f'let {kind}Source := {json.dumps(text)};\n'
            numeric_setup += (f'let {kind}Nodes := [[[[grammar parse: {kind}Source from: #json] children] at: 0] children];\n')
            numeric_cases += (f'  "{kind}_numbers" -> [| [{kind}Nodes collect: [:node | [node jsonNumber: {kind}Source]]]]\n')
        numeric_cases += '  "parse_floats" -> [| [Json parse: floatSource]]\n'
        numeric_cases += '  "parse_long_floats" -> [| [Json parse: long_floatSource]]\n'
        code = code.replace('let cases := {', numeric_setup + 'let cases := {\n' + numeric_cases)
        p.write_text(code)
        built = command([binary, 'build', project], cwd=work, env=env, timeout=120)
        if built['exit_code']:
            raise RuntimeError(built)
    evidence = {'schema': 1, 'source_sha256': before, 'fixture': manifest['valid']['small'],
                'runtime_sha256': {p.name: sha(p) for p in [binary, *sorted((prefix/'lib').glob('*.dylib'))]},
                'runner_sha256': sha(Path(__file__)), 'profile_sha256': sha(ROOT/'examples/log-summary/profile/main.birnam'),
                'method': 'ABBA per phase, one warmup and 3 timed iterations per process. Same runtime; normal GC. Independent workloads, not additive attribution.',
                'rows': []}
    for phase in phases:
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
            elif phase == 'numbers':
                assert observed == list(map(json.loads, data['number_inputs']))
            elif phase.endswith('_numbers') or phase in ('parse_floats', 'parse_long_floats'):
                kind = {'parse_floats': 'float', 'parse_long_floats': 'long_float'}.get(
                    phase, phase.removesuffix('_numbers'))
                assert observed == numeric_expected[kind]
            elif phase in ('unescape', 'decode_strings'):
                assert observed == list(map(json.loads, data['string_inputs']))
            else:
                assert observed == ['\n']*250
            evidence['rows'].append({'variant': label, 'phase': phase, 'samples': row['samples']})
            save_json(work/'results.json', evidence)
            print(phase, label, round(statistics.median(s['cpu_ns'] for s in row['samples'])/1e6, 3), flush=True)
    evidence['medians'] = {}
    for phase in phases:
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
