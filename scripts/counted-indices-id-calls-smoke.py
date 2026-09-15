#!/usr/bin/env python3
"""Check counted-index allocation savings with ordinary Id driving numeric calls."""
from validation_support import save_json
from measurement_archive import load_json, read_bytes
import argparse
import hashlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def module(name, filename):
    spec = importlib.util.spec_from_file_location(name, ROOT / 'scripts' / filename)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


timing = module('counted_timing', 'counted-indices-production-timing.py')
aggregate = module('counted_aggregate', 'counted-frame-aggregate-smoke.py')


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def source(operations, shape, precise):
    # Keep the production numeric bodies and per-call value/identity checker
    # identical. Four Id calls per wrapper keep eight measured wrappers below
    # the existing automatic-collection threshold, without changing that limit.
    setup = timing.source(operations, shape, precise).split('let coldStart :=', 1)[0]
    mutation = f'''[Reflect defineMethod: #+ on: input as: [:self :right | [effects inc]; [11 - right]]];
[expected set: {11 + 3 * (operations - 2)}];
''' if shape == 1 else ''
    return setup + mutation + '''let marker := 42;
let wrapperSource := [:unused :alsoUnused | [4 timesRepeat: invoke]; marker];
let wrapper := [Codegen materializeResumable: [[wrapperSource codeObject] withoutSourceProvenance] captures: #[invoke marker]];
[UnboxedNumericProbe run: wrapper input: block other: other expected: 42];
[check value: [last value]];
''' + f'''[[[effects value] = {544 if shape == 1 else 0}] ifTrue: [| nil] ifFalse: [| [OS exit: 71]]];
''' + '''[Codegen evictMaterializations];
[System gc];
[check value: [last value]];
[invoke value];
[System println: "counted-id-calls-ok"];
'''


def parsed(row):
    output = row['stdout']
    assert output.splitlines().count('counted-id-calls-ok') == 1, row
    for prefix, key in (('unboxed-numeric ', 'measurements'), ('unboxed-numeric-gc ', 'collection')):
        lines = [line for line in output.splitlines() if line.startswith(prefix)]
        assert len(lines) == 1, row
        row[key] = {'id_calls': {k: int(v) for k, v in re.findall(r'(\w+)=(-?\d+)', lines[0])}}
    m, gc = row['measurements']['id_calls'], row['collection']['id_calls']
    assert m['invocations'] == 8 and m['answer'] == 42 and m['bytes'] > 0
    assert gc['live_objects'] == m['live_before'] + m['logical_allocations'] - gc['reclaimed_objects']
    assert m['counted_indices'] >= 0 and (row['counted'] or m['counted_indices'] == 0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path, required=True)
    parser.add_argument('--receipt', type=Path, required=True)
    parser.add_argument('--profile', choices=('debug', 'release'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), 'preserve existing evidence'
    proof = load_json(args.receipt)
    assert proof['status'] == proof['production_verification'] == 'passed'
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    assert sha(floor) == proof['unboxed_source_matrices'][f'unboxed-measure-{args.profile}.json']['floor_sha256']
    sources = dict(proof['sources'])
    for name in ('counted-indices-id-calls-smoke.py', 'counted-indices-production-timing.py'):
        sources['scripts/' + name] = sha(ROOT / 'scripts' / name)
    manifest = prefix / 'share/birnam/rust-install.json'
    installed = load_json(manifest)
    assert installed['profile'] == 'release' and installed['publication']['state'] == 'published'
    artifacts = dict(installed['staged_manifest'])
    artifacts['share/birnam/rust-install.json'] = sha(manifest)
    shipped = {n.removeprefix('share/birnam/src/'): h for n, h in artifacts.items()
               if n.startswith('share/birnam/src/')}
    assert shipped and all(sha(ROOT / n) == h for n, h in shipped.items())
    sources.update(shipped)
    result = dict(status='running', qualified=False, profile=args.profile, sources=sources,
                  prefix=str(prefix), installed_sha256=artifacts, floor=str(floor), floor_sha256=sha(floor),
                  proof_sha256=sha(args.receipt), rows=[], attempts=[],
                  scope='Counter composition for ordinary Id invocation, not timing. Production numeric bodies and per-call value/identity checks are reused; a resumable wrapper invokes them four times through ordinary timesRepeat:/value:. The existing probe warms128wrappers then measures8wrappers/32numeric calls, preserving its allocation threshold and caller roots. Both GC modes, all five shapes, three chain lengths, unboxing off/on and counting off/on. Matched-start pairs retain every attempt; equal-start outcome differences fail immediately. Id driver overhead is included in total savings; no claim that all savings belong to the numeric body alone.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_', 'BENCH_HOST_'))}
    env.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_UNBOXED_NUMERIC='1', BIRNAM_NUMERIC_BATCHES='2')

    def save():
        save_json(temporary, result)

    def current():
        assert all(sha(ROOT / n) == h for n, h in sources.items()), 'source changed'
        assert all(sha(prefix / n) == h for n, h in artifacts.items()), 'installation changed'
        assert sha(floor) == result['floor_sha256'] and sha(args.receipt) == result['proof_sha256']

    try:
        current()
        save()
        with tempfile.TemporaryDirectory(prefix='birnam-counted-id-calls-') as work:
            fixture = Path(work) / 'calls.birnam'
            for n, s, gc, u in itertools.product((3, 16, 32), range(5), (0, 1), (0, 1)):
                code = source(n, s, gc)
                fixture.write_text(code)
                for attempt_number in range(3):
                    attempt = dict(operations=n, shape=s, precise=gc, unboxed=u,
                                   attempt=attempt_number, status='running', rows=[])
                    result['attempts'].append(attempt)
                    for counted in (0, 1):
                        current()
                        p = subprocess.run([str(prefix / 'bin/birnam'), str(fixture)], cwd=ROOT,
                            env=dict(env, BIRNAM_NUMERIC_SHAPE=str(s), BIRNAM_DEV_UNBOXED_NUMERIC=str(u),
                                     BIRNAM_DEV_COUNTED_FRAME_INDICES=str(counted)),
                            text=True, capture_output=True, timeout=180)
                        row = dict(operations=n, shape=s, precise=gc, unboxed=u, counted=counted,
                                   source=code, fixture_sha256=hashlib.sha256(code.encode()).hexdigest(),
                                   exit_code=p.returncode, stdout=p.stdout, stderr=p.stderr)
                        attempt['rows'].append(row)
                        save()
                        assert p.returncode == 0, row
                        parsed(row)
                        save()
                    if not aggregate.compare_pair(*attempt['rows']):
                        attempt['status'] = 'incomparable_starting_heaps'
                        save()
                        continue
                    if s in (0, 4):
                        assert attempt['rows'][1]['measurements']['id_calls']['counted_indices'] > 0
                    attempt['status'] = 'passed'
                    result['rows'].extend(attempt['rows'])
                    save()
                    break
                else:
                    raise AssertionError('no matched starting heaps after three attempts')
                print(n, s, gc, u, 'Id calls passed', flush=True)
        current()
        assert len(result['rows']) == 120
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
