#!/usr/bin/env python3
"""Real V2 capture/suspension of repeated literal results, without scratch GC roots."""
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
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    fixture = root / 'scripts/fixtures/closure-literal-control.birnam'
    paths = [fixture, Path(__file__).resolve()]
    paths += sorted((root / 'rust/birnam-codegen/src').rglob('*.rs'))
    paths += sorted((root / 'rust/birnam-id/src').rglob('*'))
    digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
    hashes = {str(p.relative_to(root)): digest(p) for p in paths if p.is_file()}
    installed = {str(p.relative_to(args.prefix)): digest(p) for p in sorted(args.prefix.rglob('*')) if p.is_file()}
    floor_hash = digest(args.floor)
    receipt = dict(status='running', source_sha256=hashes, floor_sha256=floor_hash, installed_sha256=installed, rows=[])
    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(args.floor.resolve()), BIRNAM_TEST_CLOSURE_CONTROL='1',
               BIRNAM_DEV_CLOSURE_REPORT='1', BIRNAM_DEV_CLOSURE_FORCE_RECOVERY='0')
    modes = [(0, 1, -1), (1, 0, -1), (1, 1, -1), (1, 1, 0), (1, 1, 1), (1, 1, 2)]
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
                expected = (3 if forced < 0 else forced) if literals and enabled else 0
                assert reports[0].startswith(f'closure-literal-control: calls={expected} fresh_results=3 supplied_distinct=true'), reports
                output = [re.sub(r'calls=\d+', 'calls=CHECKED', line) for line in lines if not line.startswith('closure-elision:')]
                if baseline is None:
                    baseline = output
                assert output == baseline, (output, baseline)
                receipt['rows'].append(dict(precise=precise, control=control, literals=literals, enabled=enabled,
                                            forced=forced, completed_calls=expected, output=output))
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
