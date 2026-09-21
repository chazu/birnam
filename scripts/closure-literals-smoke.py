#!/usr/bin/env python3
"""Differential installed source checks for fresh literal closure results."""
from measurement_archive import read_bytes
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--floor', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--numeric-body', action='store_true', help='Exercise two arithmetic sends in each closure body')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    fixture = root / 'scripts/fixtures/closure-literals.birnam'
    if args.numeric_body:
        fixture = root / 'scripts/fixtures/closure-numeric.birnam'
    paths = [fixture, Path(__file__).resolve()]
    paths += sorted((root / 'rust/birnam-codegen/src').rglob('*.rs'))
    paths += sorted((root / 'rust/birnam-id/src').rglob('*'))
    paths += sorted((root / 'rust/birnam-runtime/src').rglob('*'))
    paths = [p for p in paths if p.is_file()]
    digest = lambda p: hashlib.sha256(read_bytes(p)).hexdigest()
    hashes = {str(p.relative_to(root)): digest(p) for p in paths}
    installed = {str(p.relative_to(args.prefix)): digest(p) for p in sorted(args.prefix.rglob('*')) if p.is_file()}
    floor_hash = digest(args.floor)
    receipt = dict(status='running', source_sha256=hashes, floor_sha256=floor_hash, installed_sha256=installed, rows=[])
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_DEV_CLOSURE_REPORT='1',
               BIRNAM_DEV_CLOSURE_FORCE_RECOVERY='0')
    try:
        baseline = None
        for precise in (0, 1):
            for literals, enabled, forced in [(0, 1, -1), (1, 0, -1), (1, 1, -1),
                                              (1, 1, 0), (1, 1, 1), (1, 1, 2)]:
                env.update(BIRNAM_CLOSURE_PRECISE=str(precise), BIRNAM_DEV_LITERAL_CALLEES=str(literals),
                           BIRNAM_DEV_CLOSURE_ELISION=str(enabled), BIRNAM_DEV_CLOSURE_FORCE_AT=str(forced))
                run = subprocess.run([str(args.prefix.resolve() / 'bin/birnam'), str(fixture)],
                                     cwd=root, env=env, text=True, capture_output=True)
                assert run.returncode == 0, (precise, literals, enabled, forced, run.stdout, run.stderr)
                lines = (run.stdout + run.stderr).splitlines()
                reports = [line for line in lines if line.startswith('closure-elision:')]
                output = [line for line in lines if not line.startswith('closure-elision:')]
                if baseline is None:
                    baseline = output
                assert output == baseline, (output, baseline)
                if (literals or args.numeric_body) and enabled:
                    if forced < 0:
                        assert 'closure-elision: completed calls=3' in reports, reports
                        assert 'closure-elision: hit materialized=true' in reports, reports
                    else:
                        assert 'closure-elision: forced constructor recovery' in reports, reports
                else:
                    assert not reports, reports
                receipt['rows'].append(dict(precise=precise, literals=literals, enabled=enabled, forced=forced,
                                            output=output, reports={line: reports.count(line) for line in set(reports)}))
                print(f'closure-literals: precise={precise} literals={literals} enabled={enabled} forced={forced} passed', flush=True)
        assert all(digest(args.prefix / name) == value for name, value in installed.items())
        assert digest(args.floor) == floor_hash
        assert all(digest(root / name) == value for name, value in hashes.items())
        receipt['status'] = 'passed'
    except BaseException as error:
        receipt.update(status='failed', error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(receipt, indent=2) + '\n')


if __name__ == '__main__':
    main()
