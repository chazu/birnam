#!/usr/bin/env python3
"""Check current numeric captures/arguments and observing arithmetic replacement."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    prefix = args.prefix.resolve()
    fixture = root / 'scripts/fixtures/closure-numeric-inputs.birnam'
    sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    paths = [Path(__file__).resolve(), fixture]
    paths += [p for p in (root / 'rust').rglob('*') if p.is_file()]
    sources = {str(p.relative_to(root)): sha(p) for p in paths}
    manifest = prefix / 'share/birnam/rust-install.json'
    publication = json.loads(manifest.read_text())
    assert publication['publication']['state'] == 'published'
    artifacts = dict(publication['staged_manifest'])
    artifacts['share/birnam/rust-install.json'] = sha(manifest)
    floor = args.floor.resolve() if args.floor else prefix / 'lib/libbirnam_id.dylib'
    floor_hash = sha(floor)
    result = dict(status='running', qualified=False, sources=sources, artifacts=artifacts,
                  floor_sha256=floor_hash, rows=[])
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_DEV_CLOSURE_REPORT='1', BIRNAM_DEV_LITERAL_CALLEES='0',
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY='0')
    if args.floor:
        env['BIRNAM_ID_DYLIB'] = str(floor)
    def current():
        assert all(sha(root / n) == h for n, h in sources.items())
        assert all(sha(prefix / n) == h for n, h in artifacts.items())
        assert sha(floor) == floor_hash
    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + '\n')
    try:
        for precise in [0, 1]:
            baseline = None
            for enabled, forced in [(0, -1), (1, -1), (1, 0), (1, 1), (1, 2)]:
                current()
                run = subprocess.run([str(prefix / 'bin/birnam'), str(fixture)], cwd=root,
                    env=dict(env, BIRNAM_CLOSURE_PRECISE=str(precise),
                             BIRNAM_DEV_CLOSURE_ELISION=str(enabled), BIRNAM_DEV_CLOSURE_FORCE_AT=str(forced)),
                    text=True, capture_output=True, timeout=180)
                lines = (run.stdout + run.stderr).splitlines()
                reports = [s for s in lines if s.startswith('closure-elision:')]
                output = [s for s in lines if not s.startswith('closure-elision:')]
                row = dict(precise=precise, enabled=enabled, forced=forced, exit_code=run.returncode,
                           stdout=run.stdout, stderr=run.stderr)
                result['rows'].append(row); save()
                assert run.returncode == 0, row
                assert output.count('numeric-inputs: changed add') == 6, row
                assert 'numeric-inputs: current values, fresh identities and changed arithmetic passed' in output, row
                if baseline is None:
                    baseline = output
                assert output == baseline, row
                if enabled and forced < 0:
                    assert 'closure-elision: completed calls=3' in reports, row
                    assert 'closure-elision: hit materialized=false' in reports, row
                elif enabled:
                    assert 'closure-elision: forced constructor recovery' in reports, row
                else:
                    assert not reports, row
                print(f'numeric inputs: precise={precise} enabled={enabled} forced={forced} passed', flush=True)
        current(); result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error)); raise
    finally:
        save()


if __name__ == '__main__':
    main()
