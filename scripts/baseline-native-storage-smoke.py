#!/usr/bin/env python3
"""Check diagnostic baseline TCC bytes across eviction, GC and live invocation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def fixture(size):
    return '''[Codegen evictMaterializations]
[System gc]
[Tcc relocatedAllocationBytes]
[Globals at: #ResourceBlock put: [Codegen materializeResumable: [CodeObject fromProgram: [CodeProgram parameters: #[] captures: #[] statements: #[[CodeStatement expression: [CodeNode string: "''' + 'x' * size + '''"]]]]] captures: #[]]]
[System gc]
[Tcc relocatedAllocationBytes]
[[[Globals at: #ResourceBlock] value] size]
[Codegen evictMaterializations]
[System gc]
[Tcc relocatedAllocationBytes]
[[[Globals at: #ResourceBlock] value] size]
[Globals at: #ResourceBlock put: nil]
[System gc]
[Tcc relocatedAllocationBytes]
:quit
'''


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', type=Path, required=True)
    p.add_argument('--target', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    args = p.parse_args()
    prefix, target = args.prefix.resolve(), args.target.resolve()
    manifest = prefix / 'share/birnam/rust-install.json'
    installed = json.loads(manifest.read_text())
    assert installed['publication']['state'] == 'published' and installed['profile'] == 'release'
    inputs = {str(path): sha(path) for path in [Path(__file__), ROOT / 'Cargo.lock']}
    for directory in ['rust', 'vendor/libtcc-src']:
        inputs.update({str(path): sha(path) for path in (ROOT / directory).rglob('*') if path.is_file()})
    artifacts = {str(prefix / name): digest for name, digest in installed['staged_manifest'].items()}
    artifacts[str(manifest)] = sha(manifest)
    modules = {profile: target / profile / 'libbirnam_tcc.dylib' for profile in ['debug', 'release']}
    artifacts.update({str(path): sha(path) for path in modules.values()})
    result = dict(status='running', scope='Diagnostic baseline TCC module overrides on a normal release installation. Owned Unit relocated allocation includes code/data/alignment, excludes compiler metadata and floor-private units. Counter expressions themselves own evaluator units, so comparisons use identical expressions after collection. Cold caches may retire with the target. No timing claim.', sources=inputs, artifacts=artifacts, rows=[])
    args.output.parent.mkdir(parents=True, exist_ok=True)
    base = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}

    def save():
        args.output.write_text(json.dumps(result, indent=2) + '\n')

    def current():
        assert all(sha(n) == h for n, h in inputs.items()), 'source changed'
        assert all(sha(n) == h for n, h in artifacts.items()), 'artifact changed'

    try:
        current()
        normal = subprocess.run([str(prefix / 'bin/birnam'), '--repl'], input='[System println: [[Tcc relocatedAllocationBytes] isError]]\n:quit\n', cwd=ROOT, env=base, capture_output=True, text=True, timeout=60)
        result['normal_protocol'] = dict(exit_code=normal.returncode, stdout=normal.stdout, stderr=normal.stderr)
        assert normal.returncode == 0 and 'birnam> true\n' in normal.stdout and not normal.stderr
        for profile, module in modules.items():
            for size in (4096, 65536):
                for repetition in range(3):
                    current()
                    source = fixture(size)
                    run = subprocess.run([str(prefix / 'bin/birnam'), '--repl'], input=source, cwd=ROOT, env=dict(base, BIRNAM_TCC_DYLIB=str(module)), capture_output=True, text=True, timeout=120)
                    row = dict(profile=profile, payload_bytes=size, repetition=repetition, exit_code=run.returncode, stdout=run.stdout, stderr=run.stderr, source_sha256=hashlib.sha256(source.encode()).hexdigest())
                    result['rows'].append(row); save()
                    assert run.returncode == 0 and not run.stderr, row
                    answers = [v for v in re.findall(r'^birnam> (.*)$', run.stdout, re.M) if v]
                    assert len(answers) == 14 and answers[3] == 'anObject' and answers[11] == 'nil', row
                    assert int(answers[6]) == int(answers[10]) == size and int(answers[7]) == 1, row
                    baseline, materialized, held, released = map(int, [answers[2], answers[5], answers[9], answers[13]])
                    assert materialized > baseline and materialized >= held >= baseline, row
                    assert released == baseline, row
                    if size == 65536:
                        assert held > released, row
                    row['relocated_allocation_bytes'] = dict(baseline=baseline, materialized=materialized, evicted_live=held, released=released)
                    save(); print('baseline-native-storage:', profile, size, repetition, 'passed', flush=True)
        current(); result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error)); raise
    finally:
        save()


if __name__ == '__main__':
    main()
