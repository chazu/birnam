#!/usr/bin/env python3
"""Check a real binding pipeline with individual and combined recovery consumers."""
from validation_support import save_json
from measurement_archive import load_json, read_bytes
import argparse
import hashlib
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
FLAGS = ('UNBOXED_NUMERIC', 'LOOP_INVARIANTS', 'CLOSURE_ELISION',
         'LITERAL_CALLEES', 'COUNTED_FRAME_INDICES', 'VIRTUAL_FIELDS', 'DISCARDED_CLONES')
EXPECTED = ['application-phase baseline', 'application-answer 45152',
            'application-changed 47584',
            'application-mutation 63152 effects=3 fresh-alias=true',
            'application-binding-pipeline passed']


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def validate(row, diagnostic):
    assert row['exit_code'] == 0 and not row['stderr'], row
    lines = row['stdout'].splitlines()
    output = [s for s in lines if not s.startswith(('application-activity ', 'application-aggregate '))]
    assert output == EXPECTED, row
    activity = [dict((k, int(v)) for k, v in
                     (part.split('=') for part in s.split()[1:]))
                for s in lines if s.startswith('application-activity ')]
    aggregates = [dict(part.split('=') for part in s.split()[1:])
                  for s in lines if s.startswith('application-aggregate ')]
    if not diagnostic:
        assert not activity and not aggregates, row
        return {}
    assert len(activity) == 2 and activity[0].keys() == activity[1].keys(), row
    assert len(aggregates) == 4, row
    delta = {k: activity[1][k] - activity[0][k] for k in activity[0]}
    assert all(v >= 0 for v in delta.values()), row
    for flag, counter in [('UNBOXED_NUMERIC', 'unboxed'),
                          ('LOOP_INVARIANTS', 'loop_reused'),
                          ('CLOSURE_ELISION', 'closure_completed'),
                          ('COUNTED_FRAME_INDICES', 'counted_indices'),
                          ('VIRTUAL_FIELDS', 'field_virtualized')]:
        assert (delta[counter] > 0) == bool(row['flags'][flag]), (flag, delta, row)
    assert delta['field_recovered'] == 0 and delta['settled_allocations'] == 0, row
    assert delta['field_virtualized'] == (64 if row['flags']['VIRTUAL_FIELDS'] else 0), row
    for family in ('clones', 'fields'):
        snapshots = [a for a in aggregates if a['family'] == family]
        assert len(snapshots) == 2, row
        for metric in ('admitted', 'completed'):
            count = int(snapshots[1][metric]) - int(snapshots[0][metric])
            assert count == (64 if family == 'fields' and row['flags']['VIRTUAL_FIELDS'] else 0), row
    return delta


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path, required=True)
    parser.add_argument('--tcc', type=Path, required=True)
    parser.add_argument('--codegen', type=Path, required=True)
    parser.add_argument('--diagnostic', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), 'preserve previous receipts'
    prefix = args.prefix.resolve()
    files = [args.floor.resolve(), args.tcc.resolve(), args.codegen.resolve()]
    sys.path.insert(0, str(ROOT / 'scripts'))
    sources = runpy.run_path(str(ROOT / 'scripts/recovery-graphs.py'))['sources']()
    for name in ('bench/recovery-binding-pipeline.birnam', 'scripts/recovery-binding-pipeline-smoke.py'):
        sources[name] = sha(ROOT / name)
    manifest_path = prefix / 'share/birnam/rust-install.json'
    manifest = load_json(manifest_path)
    assert manifest['publication']['state'] == 'published'
    artifacts = {str(p): sha(p) for p in files + [manifest_path]}
    artifacts.update({str(prefix / n): h for n, h in manifest['staged_manifest'].items()})
    result = dict(status='running', qualified=False, diagnostic=args.diagnostic,
                  sources=sources, artifacts=artifacts, rows=[],
                  scope='Ordinary application results, late getter replacement, clone identity, '
                        'individual controls and actual activity. Copied published host with '
                        'explicit module overrides; not fresh installed production acceptance.')

    def current():
        assert all(sha(ROOT / n) == h for n, h in sources.items()), 'source changed'
        assert all(sha(n) == h for n, h in artifacts.items()), 'artifact changed'

    def save():
        save_json(args.output, result)

    configs = [('off', dict.fromkeys(FLAGS, 0)), ('on', dict.fromkeys(FLAGS, 1))]
    for flag in ('UNBOXED_NUMERIC', 'LOOP_INVARIANTS', 'CLOSURE_ELISION', 'VIRTUAL_FIELDS'):
        configs.append((flag.lower() + '-off', dict(dict.fromkeys(FLAGS, 1), **{flag: 0})))
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    try:
        current(); save()
        with tempfile.TemporaryDirectory(prefix='birnam-binding-pipeline-') as tmp:
            host = Path(tmp)
            (host / 'bin').mkdir(); (host / 'lib').mkdir()
            shutil.copy2(prefix / 'bin/birnam', host / 'bin/birnam')
            (host / 'share').symlink_to(prefix / 'share', target_is_directory=True)
            overrides = {'libbirnam_id_tcc.dylib': files[1], 'libbirnam_id_codegen.dylib': files[2]}
            for file in (prefix / 'lib').iterdir():
                (host / 'lib' / file.name).symlink_to(overrides.get(file.name, file))
            for precise in (0, 1):
                for label, flags in configs:
                    current()
                    rowenv = dict(env, BIRNAM_ID_DYLIB=str(files[0]),
                                  BIRNAM_APPLICATION_PRECISE=str(precise),
                                  BIRNAM_APPLICATION_DIAGNOSTICS=str(int(args.diagnostic)),
                                  BIRNAM_TEST_CLOSURE_CONTROL=str(int(args.diagnostic)))
                    rowenv.update({'BIRNAM_DEV_' + k: str(v) for k, v in flags.items()})
                    process = subprocess.run([str(host / 'bin/birnam'), str(ROOT / 'bench/recovery-binding-pipeline.birnam')],
                                             cwd=ROOT, env=rowenv, capture_output=True, text=True, timeout=180)
                    row = dict(precise=precise, configuration=label, flags=flags,
                               exit_code=process.returncode, stdout=process.stdout, stderr=process.stderr)
                    result['rows'].append(row); save()
                    row['activity_delta'] = validate(row, args.diagnostic)
                    save(); print(precise, label, 'passed', flush=True)
        current(); result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error)); raise
    finally:
        save()


if __name__ == '__main__':
    main()
