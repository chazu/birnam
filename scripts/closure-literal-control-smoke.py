#!/usr/bin/env python3
"""Real V2 capture/suspension of repeated literal results, without scratch GC roots."""
from measurement_archive import read_bytes
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import re
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--numeric-body', action='store_true', help='Exercise two arithmetic sends in each closure body')
    parser.add_argument('--numeric-copy-region', action='store_true')
    parser.add_argument('--counted-frame-indices', action='store_true')
    args = parser.parse_args()
    if args.numeric_copy_region and not args.numeric_body:
        parser.error('--numeric-copy-region requires --numeric-body')
    root = Path(__file__).resolve().parent.parent
    fixture = root / 'scripts/fixtures/closure-literal-control.birnam'
    if args.numeric_body:
        fixture = root / 'scripts/fixtures/closure-numeric-control.birnam'
    if args.numeric_copy_region:
        fixture = root / 'scripts/fixtures/closure-counted-numeric-control.birnam'
    paths = [fixture, Path(__file__).resolve()]
    paths += sorted((root / 'rust/birnam-codegen/src').rglob('*.rs'))
    paths += sorted((root / 'rust/birnam-id/src').rglob('*'))
    paths += sorted((root / 'rust/birnam-runtime/src').rglob('*'))
    digest = lambda p: hashlib.sha256(read_bytes(p)).hexdigest()
    hashes = {str(p.relative_to(root)): digest(p) for p in paths if p.is_file()}
    installed = {str(p.relative_to(args.prefix)): digest(p) for p in sorted(args.prefix.rglob('*')) if p.is_file()}
    floor_hash = digest(args.floor)
    receipt = dict(status='running', source_sha256=hashes, floor_sha256=floor_hash, installed_sha256=installed,
                   counted_frame_indices=args.counted_frame_indices, numeric_copy_region=args.numeric_copy_region, rows=[])
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_TEST_CLOSURE_CONTROL='1',
               BIRNAM_DEV_CLOSURE_REPORT='1', BIRNAM_DEV_CLOSURE_FORCE_RECOVERY='0')
    env['BIRNAM_DEV_COUNTED_FRAME_INDICES'] = str(int(args.counted_frame_indices))
    if args.numeric_copy_region:
        env['BIRNAM_TEST_COUNTED_CLOSURE_CONTROL'] = '1'
    modes = [(0, 1, -1), (1, 0, -1), (1, 1, -1), (1, 1, 0), (1, 1, 1), (1, 1, 2)]
    if args.numeric_body:
        # The shared fresh-result probe expects its literal test mode enabled.
        # Numeric admission's independence from that flag is checked by the
        # ordinary source matrix; keep this matrix focused on V2 ownership.
        modes = [mode for mode in modes if mode[0] == 1]
    try:
        for precise, control in itertools.product((0, 1), ('capture', 'suspend')):
            baseline = None
            for literals, enabled, forced in modes:
                env.update(BIRNAM_CLOSURE_PRECISE=str(precise), BIRNAM_CLOSURE_CONTROL=control,
                           BIRNAM_DEV_LITERAL_CALLEES=str(literals), BIRNAM_DEV_CLOSURE_ELISION=str(enabled),
                           BIRNAM_DEV_CLOSURE_FORCE_AT=str(forced))
                run = subprocess.run([str(args.prefix.resolve() / 'bin/birnam'), str(fixture)],
                                     cwd=root, env=env, text=True, capture_output=True)
                assert run.returncode == 0, (precise, control, literals, enabled, forced, run.stdout, run.stderr)
                lines = (run.stdout + run.stderr).splitlines()
                reports = [line for line in lines if line.startswith('closure-literal-control: calls=')]
                assert len(reports) == 1, lines
                expected = (3 if forced < 0 else forced) if (literals or args.numeric_body) and enabled else 0
                assert reports[0].startswith(f'closure-literal-control: calls={expected} fresh_results=3 supplied_distinct=true'), reports
                counted = None
                if args.numeric_copy_region:
                    counted_lines = [line for line in lines if line.startswith('closure-counted-control: ')]
                    assert len(counted_lines) == 1, lines
                    counted = {k: int(v) for k, v in re.findall(r'(\w+)=(\d+)', counted_lines[0])}
                    assert set(counted) == {'before_pause', 'after_resume'}, counted
                    if args.counted_frame_indices:
                        assert counted['before_pause'] > 0, counted
                        assert counted['after_resume'] >= counted['before_pause'], counted
                    else:
                        assert counted['before_pause'] == counted['after_resume'] == 0, counted
                output = [re.sub(r'calls=\d+', 'calls=CHECKED', line) for line in lines
                          if not line.startswith(('closure-elision:', 'closure-counted-control: '))]
                if baseline is None:
                    baseline = output
                assert output == baseline, (output, baseline)
                receipt['rows'].append(dict(precise=precise, control=control, literals=literals, enabled=enabled,
                                            forced=forced, completed_calls=expected, counted=counted,
                                            stdout=run.stdout, stderr=run.stderr, output=output))
                print(f'closure-literal-control: precise={precise} control={control} literals={literals} enabled={enabled} forced={forced} passed', flush=True)
        assert digest(args.floor) == floor_hash
        assert all(digest(root / name) == value for name, value in hashes.items())
        assert all(digest(args.prefix / name) == value for name, value in installed.items())
        receipt['status'] = 'passed'
    except BaseException as error:
        receipt.update(status='failed', error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()
