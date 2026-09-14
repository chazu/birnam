#!/usr/bin/env python3
"""Compare main with counting disabled/enabled on verified normal installations."""
import argparse
import hashlib
import importlib.util
import itertools
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
CASES = tuple(itertools.product((3, 16, 32), range(5), (0, 1), (0, 1)))
STATES = ('before', 'disabled', 'enabled')
METRICS = ('execution_us', 'including_gc_us', 'cold_execution_us',
           'materialization_us', 'process_wall_ns', 'peak_rss_bytes')
EXPECTED_CHANGES = {
    'rust/birnam-id/src/heap.rs',
    'rust/birnam-id/src/heap/dead_allocations.rs',
    'rust/birnam-id/src/heap/dead_allocations/frame_indices.rs',
    'rust/birnam-id/src/heap/dead_allocations/numeric.rs',
    'rust/birnam-id/src/runtime/native_tier/leaf_tests/discarded_clone_tests.rs',
    'rust/birnam-id/src/runtime/native_tier/leaf_tests/region_tests.rs',
    'rust/birnam-id/src/runtime/native_tier/regions.rs',
    'rust/birnam-id/src/runtime/unboxed_numeric_probe.rs',
    'rust/birnam-id/src/runtime/unboxed_numeric_probe/measure.rs',
    'scripts/aggregate-source-smoke.py', 'scripts/bench-host-check.sh',
    'scripts/counted-frame-aggregate-smoke.py', 'scripts/counted-frame-closure-control-smoke.py',
    'scripts/counted-frame-indices-smoke.py', 'scripts/counted-indices-id-calls-smoke.py',
    'scripts/counted-indices-production-timing.py', 'scripts/loop-invariant-numeric-smoke.py',
    'scripts/recovery-graphs.py', 'scripts/unboxed-numeric-control-smoke.py',
}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('before-proof', 'before-driver', 'after-proof', 'after-driver', 'acceptance', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--batches', type=int, default=100)
    parser.add_argument('--pairs', type=int, default=3)
    args = parser.parse_args()
    if args.output.exists() or not 1 <= args.batches <= 500 or not 1 <= args.pairs <= 6:
        parser.error('preserve existing evidence and use bounded positive batches/pairs')
    spec = importlib.util.spec_from_file_location('counted_driver', ROOT / 'scripts/counted-indices-production-timing.py')
    driver = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(driver)
    paths = {}

    def bind(path, digest):
        path = str(Path(path).resolve())
        if path in paths and paths[path] != digest:
            parser.error('conflicting artifact binding: ' + path)
        if sha(path) != digest:
            parser.error('changed artifact: ' + path)
        paths[path] = digest

    for name in ('counted-indices-checkpoint-timing.py', 'counted-indices-production-timing.py',
                 'unboxed_numeric_timing_common.py', 'bench-host-check.sh'):
        bind(ROOT / 'scripts' / name, sha(ROOT / 'scripts' / name))
    checkpoints = {}
    fixtures = {f'{n}:{s}:{gc}': hashlib.sha256(driver.source(n, s, gc).encode()).hexdigest()
                for n, s, gc in itertools.product((3, 16, 32), range(5), (0, 1))}
    for name in ('before', 'after'):
        proof_path = getattr(args, name + '_proof').resolve()
        driver_path = getattr(args, name + '_driver').resolve()
        if proof_path.parent.name != 'measurements' or proof_path.parent.parent.name != 'docs':
            parser.error('production proof must be in its repository docs/measurements')
        root = proof_path.parents[2]
        proof = json.loads(proof_path.read_text())
        if (proof.get('status') != 'passed' or proof.get('production_verification') != 'passed'
                or len(proof.get('commands', [])) != 75
                or any(r['exit_code'] for r in proof['commands'])):
            parser.error(name + ' production gate incomplete')
        verified = json.loads(driver_path.read_text())
        if verified.get('status') != 'passed' or verified.get('batches') != 2:
            parser.error(name + ' driver acceptance incomplete')
        if name == 'before':
            if verified['proof_sha256'] != sha(proof_path) or verified['sources'] != proof['sources']:
                parser.error('baseline proof differs from acceptance')
            for path, digest in verified['driver_sources'].items():
                bind(path, digest)
            expected = {(n, s, gc, u, c, False) for n, s, gc, u in CASES for c in (0, 1)}
            installed_key = 'installed'
        else:
            if (verified.get('verification') is not True or verified['correctness_sha256'] != sha(proof_path)
                    or verified['fixture_sha256'] != fixtures
                    or verified['test_floor_sha256'] != proof['unboxed_source_matrices']['unboxed-measure-release.json']['floor_sha256']):
                parser.error('candidate proof, fixtures or diagnostic floor differs from acceptance')
            expected = {(n, s, gc, u, c, test) for n, s, gc, u in CASES
                        for c, test in itertools.product((0, 1), (False, True))}
            installed_key = 'installed_sha256'
        rows = verified['rows']
        if len(rows) != len(expected) or {
                (r['operations'], r['shape'], r['precise'], r['unboxed'], r['counted'], r['test_floor'])
                for r in rows} != expected:
            parser.error(name + ' incomplete driver coordinates')
        for row in rows:
            if row['exit_code']:
                parser.error(name + ' failed driver process')
            parsed = driver.parsed(row['stdout'], 2)
            if any(row[k] != value for k, value in parsed.items()):
                parser.error(name + ' inconsistent parsed driver output')
            if name == 'before':
                source = driver.source(row['operations'], row['shape'], row['precise'])
                if row['source'] != source or row['fixture_sha256'] != hashlib.sha256(source.encode()).hexdigest():
                    parser.error('baseline fixture differs from candidate')
        prefix = Path(verified['prefix']).resolve()
        manifest_path = prefix / 'share/birnam/rust-install.json'
        manifest = json.loads(manifest_path.read_text())
        installed = dict(manifest['staged_manifest'])
        installed['share/birnam/rust-install.json'] = sha(manifest_path)
        if (manifest['profile'] != 'release' or manifest['publication']['state'] != 'published'
                or installed != verified[installed_key]):
            parser.error(name + ' normal installation differs from accepted artifacts')
        shipped = {n.removeprefix('share/birnam/src/'): h for n, h in installed.items()
                   if n.startswith('share/birnam/src/')}
        if not shipped:
            parser.error('missing installed sources')
        for mapping in (proof['sources'], verified['sources'], shipped):
            for path, digest in mapping.items():
                bind(root / path, digest)
        for path, digest in installed.items():
            bind(prefix / path, digest)
        bind(proof_path, sha(proof_path))
        bind(driver_path, sha(driver_path))
        checkpoints[name] = dict(root=str(root), prefix=str(prefix), proof=str(proof_path),
                                 driver=str(driver_path), sources=proof['sources'])
    before, after = (checkpoints[n]['sources'] for n in ('before', 'after'))
    changed = {n for n in before.keys() | after.keys() if before.get(n) != after.get(n)}
    if changed != EXPECTED_CHANGES:
        parser.error('production source differences exceed or differ from the counted main composition')
    acceptance = json.loads(args.acceptance.read_text())
    if (acceptance.get('status') != 'passed' or acceptance['proof_sha256'] != sha(args.after_proof)
            or acceptance['sources'] != after):
        parser.error('exact candidate focused acceptance must complete first')
    expected_steps = {f'{name}-{profile}': count for profile in ('debug', 'release')
                      for name, count in (('numeric', 60), ('id-calls', 120), ('aggregate', 240),
                                          ('numeric-v2', 448), ('loop', 32), ('closure-v2', 24))}
    expected_steps['normal-driver'] = 240
    steps = acceptance['steps']
    if (len(steps) != 14 or any(s['status'] != 'passed' or s['exit_code'] for s in steps)
            or {s['label'] for s in steps} != set(expected_steps) | {'install'}):
        parser.error('candidate acceptance pipeline incomplete')
    for step in steps:
        if step['label'] == 'install':
            continue
        receipt = Path(step['receipt'])
        bind(receipt, step['receipt_sha256'])
        data = json.loads(receipt.read_text())
        if data['status'] != 'passed' or len(data['rows']) != expected_steps[step['label']]:
            parser.error('focused matrix incomplete: ' + step['label'])
        if step['label'] == 'normal-driver' and receipt.resolve() != args.after_driver.resolve():
            parser.error('candidate normal driver differs from completed queue')
    bind(args.acceptance, sha(args.acceptance))
    result = dict(status='running', qualified=False, checkpoints=checkpoints, sha256=paths,
                  fixture_sha256=fixtures, batches=args.batches, pairs=args.pairs,
                  rows=[], host_observations=[], production_source_changes=sorted(changed),
                  scope='Direct main baseline, candidate counting disabled and candidate counting enabled. Normal published floors and identical verified ordinary Id fixtures; no diagnostic floor or native probe. Each three-state group rotates execution order across pairs (all positions covered with three pairs). Includes reverse numeric-journal lookup as well as counted indices; disabled/baseline results cannot attribute cost solely to the live environment query. Six metrics include Id checks and driver overhead. Post-group host snapshots do not qualify intervals; always exploratory under the quiet-window waiver.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_', 'BENCH_HOST_'))}

    def current():
        if any(sha(path) != digest for path, digest in paths.items()):
            raise RuntimeError('bound source, installation, fixture generator or receipt changed')

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        temporary = args.output.with_suffix('.json.tmp')
        temporary.write_text(json.dumps(result, indent=2) + '\n')
        temporary.replace(args.output)

    try:
        current()
        save()
        with tempfile.TemporaryDirectory(prefix='birnam-counted-checkpoints-') as work:
            fixture = Path(work) / 'numeric.birnam'
            for n, s, gc, u in CASES:
                fixture.write_text(driver.source(n, s, gc))
                for pair in range(args.pairs):
                    order = STATES[pair % 3:] + STATES[:pair % 3]
                    if pair >= 3:
                        order = order[::-1]
                    for state in order:
                        current()
                        checkpoint = checkpoints['before' if state == 'before' else 'after']
                        extra = dict(BIRNAM_NUMERIC_SHAPE=str(s), BIRNAM_NUMERIC_BATCHES=str(args.batches),
                                     BIRNAM_DEV_UNBOXED_NUMERIC=str(u), BIRNAM_DEV_COUNTED_FRAME_INDICES=str(int(state == 'enabled')))
                        start = time.monotonic_ns()
                        p = subprocess.run(['/usr/bin/time', '-l', str(Path(checkpoint['prefix']) / 'bin/birnam'), str(fixture)],
                                           cwd=checkpoint['root'], env=dict(env, **extra), capture_output=True,
                                           text=True, timeout=1800)
                        row = dict(state=state, operations=n, shape=s, precise=gc, unboxed=u, pair=pair,
                                   exit_code=p.returncode, stdout=p.stdout, stderr=p.stderr,
                                   process_wall_ns=time.monotonic_ns() - start)
                        result['rows'].append(row)
                        save()
                        if p.returncode:
                            raise RuntimeError('normal checkpoint process failed')
                        row.update(driver.parsed(p.stdout, args.batches), peak_rss_bytes=driver.peak_rss(p.stderr))
                        save()
                        print(n, s, gc, u, pair, state, 'passed', flush=True)
                    p = subprocess.run(['bash', 'scripts/bench-host-check.sh'], cwd=ROOT, env=env,
                                       capture_output=True, text=True)
                    result['host_observations'].append(dict(operations=n, shape=s, precise=gc, unboxed=u,
                        pair=pair, exit_code=p.returncode, output=p.stdout + p.stderr))
                    save()
        current()
        result['comparisons'] = []
        for n, s, gc, u in CASES:
            for first, second in (('before', 'disabled'), ('before', 'enabled'), ('disabled', 'enabled')):
                for metric in METRICS:
                    changes = []
                    for pair in range(args.pairs):
                        rows = {r['state']: r for r in result['rows'] if
                                (r['operations'], r['shape'], r['precise'], r['unboxed'], r['pair']) == (n, s, gc, u, pair)}
                        if rows[first][metric] > 0:
                            changes.append(100 * (1 - rows[second][metric] / rows[first][metric]))
                    if changes:
                        result['comparisons'].append(dict(operations=n, shape=s, precise=gc, unboxed=u,
                            before=first, after=second, metric=metric, paired_reduction_percent=changes,
                            median_reduction_percent=statistics.median(changes)))
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
