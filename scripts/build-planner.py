#!/usr/bin/env python3
"""Compare Store facts with persistent Map/Vector snapshots on the same planner."""
import argparse
import json
from pathlib import Path
import platform
import re
import shutil
import subprocess
from build_planner_oracle import fixture, expected
from validation_support import ROOT, clean_environment, command, save_json, sha


def check_output(output, data):
    lines = output.splitlines()
    if not lines or lines[-1] != 'done':
        raise ValueError('missing completion marker')
    rows = [json.loads(line) for line in lines[:-1]]
    wanted, past = expected(data)
    actual = [r for r in rows if r['kind'] == 'revision']
    history = [r for r in rows if r['kind'] == 'historical']
    if len(actual) != len(wanted) or len(history) != len(past):
        raise ValueError('wrong revision/history count')
    for got, want in zip(actual+history, wanted+past):
        for key, value in want.items():
            if got.get(key) != value:
                raise ValueError(f"revision {got['index']} {got['kind']}: wrong {key}")
    memory = [r for r in rows if r['kind'] == 'memory']
    if [r['index'] for r in memory] != list(range(-1, len(wanted))):
        raise ValueError('missing memory checkpoints')
    if len(rows) != len(actual)+len(history)+len(memory):
        raise ValueError('unexpected output row')
    for row in actual:
        required = {'edit_read', 'edit', 'read', 'plan', 'checkpoint', 'publish'}
        if data['revisions'][row['index']]['restore'] is not None:
            required.add('restore_read')
        if set(row['times']) != required or any(v <= 0 for t in row['times'].values() for v in t.values()):
            raise ValueError('invalid phase timings')
    return rows


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', type=Path, required=True)
    p.add_argument('--work', type=Path, required=True)
    p.add_argument('--shapes', default='chain,wide,shared')
    p.add_argument('--sizes', default='8,32,128')
    p.add_argument('--history', type=int, default=12)
    p.add_argument('--samples', type=int, default=3)
    p.add_argument('--timeout', type=float, default=300)
    args = p.parse_args()
    shapes = args.shapes.split(','); sizes = [int(s) for s in args.sizes.split(',')]
    if platform.system() != 'Darwin': p.error('clock IDs currently require macOS')
    if (any(s not in ('chain', 'wide', 'shared') for s in shapes) or len(set(shapes)) != len(shapes)
        or any(not 4 <= n <= 20000 for n in sizes) or len(set(sizes)) != len(sizes)
        or not 10 <= args.history <= 1000 or not 1 <= args.samples <= 10 or not 1 <= args.timeout <= 1800):
        p.error('invalid shapes, sizes (4..20000), history (10..1000), samples (1..10), or timeout (1..1800)')
    prefix = args.prefix.resolve(); work = args.work.resolve(); binary = prefix/'bin/birnam'
    if not binary.is_file(): p.error('prefix has no bin/birnam')
    if work == ROOT or ROOT.is_relative_to(work) or work == prefix or prefix.is_relative_to(work):
        p.error('work must be a dedicated directory')
    marker = work/'build-planner-work.json'
    if work.exists() and not marker.is_file(): p.error('work directory is not owned by this runner')
    work.mkdir(parents=True, exist_ok=True); save_json(marker, {'purpose': 'build planner comparison'})
    project = work/'app'; project.mkdir(exist_ok=True)
    files = sorted((ROOT/'examples/build-planner').glob('*.birnam'))
    for f in files: shutil.copy2(f, project/f.name)
    bound = files+[Path(__file__).resolve(), ROOT/'scripts/build_planner_oracle.py',
                   ROOT/'scripts/build-planner-test.py', ROOT/'scripts/validation_support.py']
    receipt = dict(schema=1, revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
                   prefix=str(prefix), platform=platform.platform(),
                   source_sha256={str(f.relative_to(ROOT)):sha(f) for f in bound if f.is_file()},
                   runtime_sha256={str(f.relative_to(prefix)):sha(f) for f in
                     [binary, *sorted((prefix/'lib').glob('*.dylib')), *sorted((prefix/'lib').glob('*.image'))]},
                   qualification='Fresh process per backend/sample, alternating order. Same graph policy. Normal GC, explicit collection between revisions. Loading, fixture parsing, full-state validation output and between-revision GC excluded from phase CPU. Heap diagnostics exclude native fact backing storage, which is reported separately. Peak RSS includes startup.',
                   runs=[], all_passed=False)
    result = work/'results.json'; save_json(result, receipt)
    for shape in shapes:
        for size in sizes:
            data = fixture(shape, size, args.history)
            fixture_path = project/f'{shape}-{size}.json'; save_json(fixture_path, data)
            for sample in range(args.samples):
                for backend in (('map','store') if sample % 2 == 0 else ('store','map')):
                    label = f'{shape}-{size}-{backend}-{sample}'
                    (project/'entry.birnam').write_text('[Globals at: #BuildPlannerConfig put: {#fixture -> "'+fixture_path.name+'" #backend -> "'+backend+'"}];\n[System load: "run.birnam"]\n')
                    metrics = work/(label+'.time'); log = work/(label+'.log')
                    row = dict(label=label, shape=shape, size=size, sample=sample, backend=backend,
                               fixture_sha256=sha(fixture_path), log=str(log))
                    try:
                        run = command(['/usr/bin/time','-l','-o',metrics,binary,'entry.birnam'],
                                      cwd=project, env=clean_environment(), timeout=args.timeout)
                        log.write_text(run['output']); row.update(exit_code=run['exit_code'], wall_ns=run['wall_ns'], status='completed')
                        if run['exit_code'] != 0 or run['stderr']: raise ValueError('process failed: '+run['output'][-2000:])
                        row['rows'] = check_output(run['stdout'], data)
                        row['correctness_passed'] = True
                    except subprocess.TimeoutExpired as exc:
                        partial = exc.stdout or ''; log.write_text(partial.decode(errors='replace') if isinstance(partial,bytes) else partial)
                        row.update(status='timeout', deadline_seconds=args.timeout, correctness_passed=False)
                    except ValueError as exc:
                        row.update(status='failed', error=str(exc), correctness_passed=False)
                    if metrics.exists():
                        m = re.search(r'(\d+)\s+maximum resident set size', metrics.read_text())
                        if m: row['peak_rss_bytes'] = int(m[1])
                    if log.exists(): row['log_sha256'] = sha(log)
                    receipt['runs'].append(row); save_json(result, receipt)
                    print(label, row['status'], row.get('error',''), flush=True)
                    if row['status'] == 'failed': raise SystemExit(1)
    receipt['all_passed'] = all(r.get('correctness_passed',False) for r in receipt['runs'])
    save_json(result, receipt)
    print(result)
    if not receipt['all_passed']: raise SystemExit(1)


if __name__ == '__main__':
    main()
