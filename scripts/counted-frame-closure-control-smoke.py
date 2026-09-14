#!/usr/bin/env python3
"""Run existing closure V2 capture/suspend checks with counted Frame indices."""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path, required=True)
    parser.add_argument('--profile', choices=('debug', 'release'), required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert not args.output.exists(), 'preserve existing evidence'
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    fixture = ROOT / 'scripts/fixtures/closure-control.birnam'
    paths = [Path(__file__).resolve(), fixture, ROOT / 'scripts/closure-control-smoke.sh']
    paths += sorted((ROOT / 'rust/birnam-id').rglob('*.rs'))
    installed = [prefix / 'bin/birnam', prefix / 'share/birnam/rust-install.json']
    installed += sorted((prefix / 'lib').glob('*.dylib'))
    result = dict(status='running', qualified=False, profile=args.profile,
                  floor=str(floor), floor_sha256=sha(floor),
                  sources={str(p.relative_to(ROOT)): sha(p) for p in paths},
                  installed={str(p): sha(p) for p in installed}, rows=[],
                  scope='24 source processes: both GC modes, capture/suspend, closure baseline/enabled/forced constructor recovery, counted indices off/on. Retains the existing closure-control-smoke.sh diagnostic and normalized-output assertions. No timing claim or proof of all closure shapes.')
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_CLOSURE_CONTROL='1', BIRNAM_DEV_CLOSURE_REPORT='1')

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + '\n')

    def current():
        assert sha(floor) == result['floor_sha256']
        assert all(sha(ROOT / n) == h for n, h in result['sources'].items())
        assert all(sha(n) == h for n, h in result['installed'].items())

    try:
        save()
        for precise, control in itertools.product((0, 1), ('capture', 'suspend')):
            baseline = None
            for counted, mode in itertools.product((0, 1), ('baseline', 'enabled', 'forced')):
                current()
                env.update(BIRNAM_CLOSURE_PRECISE=str(precise), BIRNAM_CLOSURE_CONTROL=control,
                           BIRNAM_DEV_CLOSURE_ELISION=str(int(mode != 'baseline')),
                           BIRNAM_DEV_CLOSURE_FORCE_RECOVERY=str(int(mode == 'forced')),
                           BIRNAM_DEV_COUNTED_FRAME_INDICES=str(counted))
                argv = [str(prefix / 'bin/birnam'), str(fixture)]
                process = subprocess.run(argv, cwd=ROOT, env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                row = dict(precise=precise, control=control, counted=counted, mode=mode,
                           argv=argv, exit_code=process.returncode, output=process.stdout)
                result['rows'].append(row)
                save()
                assert process.returncode == 0, row
                lines = process.stdout.splitlines()
                begin = lines.index('closure-control: begin measured control')
                end = lines.index('closure-control: end measured control')
                diagnostics = [line for line in lines[begin + 1:end] if line.startswith('closure-elision: ')]
                expected = ([] if mode == 'baseline' else
                            ['closure-elision: hit materialized=true', 'closure-elision: completed calls=3']
                            if mode == 'enabled' else ['closure-elision: forced constructor recovery'])
                assert diagnostics == expected, row
                normalized = [line for line in lines if not line.startswith('closure-elision: ')]
                if baseline is None:
                    baseline = normalized
                assert normalized == baseline, row
            print(f'counted-frame-closure-control: profile={args.profile} precise={precise} control={control} passed', flush=True)
        current()
        assert len(result['rows']) == 24
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        save()


if __name__ == '__main__':
    main()
