#!/usr/bin/env python3
"""Check active synchronous code ownership under precise test-probe collection."""
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
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(read_bytes(Path(path))).hexdigest()


def fixtures():
    source = runpy.run_path(str(ROOT / 'scripts/aggregate-source-smoke.py'))['source']
    logicvar = source(3, 3, True)
    error = source(16, 3, False).replace('[LogicVar named: expected]', '[Error kind: 7 message: expected]').replace(' name]', ' message]').replace('#name', '#message')
    setup = logicvar.splitlines()[0] + '\n'
    invocation = '[UnboxedNumericProbe run: block input: proto other: observer expected: 42]'
    return {
        'probe-root-boundary': '[UnboxedNumericProbe checkNativeRoots]\n',
        'logicvar-original': logicvar,
        'error-original': error,
        'nested-sync': setup + '[Codegen eval: ' + json.dumps(invocation) + ']\n',
        'nested-sync-block': setup + '[Codegen eval: ' + json.dumps('let local := [| 42]; ' + invocation + '; [local value]') + ']\n',
        'nested-persisting': setup + '[Codegen evalPersisting: ' + json.dumps(invocation) + ']\n',
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--target', type=Path, required=True, help='Codegen target built with test-evaluator-lifetime in both profiles')
    parser.add_argument('--floor-root', type=Path, required=True)
    parser.add_argument('--floor-proof', type=Path, help='Optional prior full floor proof; omitted for focused development checks')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument("--profiles", nargs="+", choices=("debug", "release"), default=["debug", "release"])
    args = parser.parse_args()
    prefix = args.prefix.resolve()
    proof = load_json(args.floor_proof) if args.floor_proof else None
    if proof:
        assert proof['status'] == proof['production_verification'] == 'passed'
    manifest = prefix / 'share/birnam/rust-install.json'
    published = load_json(manifest)
    assert published['publication']['state'] == 'published'
    artifacts = {str(prefix / name): digest for name, digest in published['staged_manifest'].items()}
    artifacts.update({str(p): sha(p) for p in [manifest] + ([args.floor_proof.resolve()] if args.floor_proof else [])})
    sources = {str(p): sha(p) for p in [Path(__file__), ROOT / 'scripts/aggregate-source-smoke.py', ROOT / 'Cargo.lock']}
    sources.update({str(p): sha(p) for p in (ROOT / 'rust').rglob('*') if p.is_file()})
    result = dict(status='running', floor_production_verification='passed' if proof else 'not_run', scope='Diagnostic codegen modules with test-evaluator-lifetime assertions in copied host layouts. Test floors preserve caller native roots while excluding probe scratch. No production-installation or timing claim.', sources=sources, artifacts=artifacts, rows=[])
    args.output.parent.mkdir(parents=True, exist_ok=True)

    def save():
        save_json(args.output, result)

    def current():
        assert all(sha(n) == h for n, h in sources.items()), 'source changed'
        assert all(sha(n) == h for n, h in artifacts.items()), 'artifact changed'

    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    try:
        for profile in args.profiles:
            floor = (args.floor_root / profile / 'libbirnam_id.dylib').resolve()
            module = (args.target / profile / 'libbirnam_codegen_runtime.dylib').resolve()
            if proof:
                assert sha(floor) == proof['unboxed_source_matrices'][f'unboxed-measure-{profile}.json']['floor_sha256']
            artifacts.update({str(p): sha(p) for p in [floor, module]})
            current()
            with tempfile.TemporaryDirectory(prefix='birnam-evaluator-lifetime-') as tmp:
                layout = Path(tmp)
                (layout / 'bin').mkdir()
                (layout / 'lib').mkdir()
                shutil.copy2(prefix / 'bin/birnam', layout / 'bin/birnam')
                (layout / 'share').symlink_to(prefix / 'share', target_is_directory=True)
                for original in (prefix / 'lib').iterdir():
                    (layout / 'lib' / original.name).symlink_to(module if original.name == 'libbirnam_id_codegen.dylib' else original)
                for name, source in fixtures().items():
                    current()
                    process = subprocess.run([str(layout / 'bin/birnam'), '--repl'], input=source + '\n:quit\n', cwd=ROOT, env=dict(env, BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_UNBOXED_NUMERIC='1', BIRNAM_DEV_DISCARDED_CLONES='0', BIRNAM_DEV_VIRTUAL_FIELDS='0', RUST_BACKTRACE='1'), capture_output=True, text=True, timeout=180)
                    row = dict(profile=profile, case=name, source=source, source_sha256=hashlib.sha256(source.encode()).hexdigest(), exit_code=process.returncode, stdout=process.stdout, stderr=process.stderr)
                    result['rows'].append(row); save()
                    assert process.returncode == 0 and not process.stderr, row
                    expected = 0 if name == 'probe-root-boundary' else 4 if name.endswith('-original') else 1
                    assert process.stdout.count('unboxed-numeric invocations=8') == expected, row
                    if name == 'probe-root-boundary':
                        assert 'source-probe-roots inner_dead=1' in process.stdout, row
                    if name.startswith('nested-'):
                        value = '42' if name == 'nested-sync-block' else 'nil'
                        assert '\n' + value + '\nbirnam>' in process.stdout, row
                    if name.endswith('-original'):
                        assert 'aggregate-source identities and current effects passed' in process.stdout, row
                    print(f'evaluator lifetime: {profile} {name} passed', flush=True)
        current(); result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error)); raise
    finally:
        save()


if __name__ == '__main__':
    main()
