#!/usr/bin/env python3
"""Sample owned normal-floor REPLs at explicit collection/eviction checkpoints."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import subprocess
import time

from closure_timing_common import source as closure_source
from loop_timing_common import source as loop_source
from unboxed_numeric_timing_common import source as numeric_source

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('aggregate_source', ROOT / 'scripts/aggregate-source-smoke.py')
aggregate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(aggregate)
SWITCHES = {'unboxed': 'BIRNAM_DEV_UNBOXED_NUMERIC', 'loop': 'BIRNAM_DEV_LOOP_INVARIANTS',
            'clones': 'BIRNAM_DEV_DISCARDED_CLONES', 'fields': 'BIRNAM_DEV_VIRTUAL_FIELDS',
            'closure': 'BIRNAM_DEV_CLOSURE_ELISION'}
DEFAULT_ON = {'clones', 'fields'}


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def phases(family, refused):
    # Factory bodies come from the separately verified counter fixtures. The
    # surrounding Id driver is new and checked in every process below.
    operations = 32 if family == 'closure' else 16
    env = {}
    common = '''[System load: "modules/os.birnam"]; [System preciseGc: true];
let held := [nil asCell]; let invoke := [nil asCell]; let lastAnswer := [nil asCell];
'''
    if family == 'unboxed':
        env.update(BIRNAM_NUMERIC_SHAPE=str(2 if refused else 0), BIRNAM_NUMERIC_BATCHES='1')
        setup = numeric_source(operations).split('let block :=', 1)[0]
        setup += 'let input := 11.5;' if refused else 'let input := 11;'
        construct = 'let block := [Codegen materializeResumable: [[source codeObject] withoutSourceProvenance] captures: #[]];'
        call, expected, identity = '[block value: input value: 3]', '59.5' if refused else '59', False
    elif family == 'closure':
        env.update(BIRNAM_CLOSURE_SHAPE=str(3 if refused else 0), BIRNAM_CLOSURE_BATCHES='1')
        setup = closure_source(operations).split('let block :=', 1)[0] + 'let input := [73 asCell];'
        construct = 'let block := [Codegen materializeResumable: [[source codeObject] withoutSourceProvenance] captures: #[]];'
        call, expected, identity = '[block value: input]', '73' if refused else 'input', not refused
    elif family in ('clones', 'fields'):
        # One discarded clone is below admission; a tail getter is an existing
        # refused virtual-field shape. Do not invent an effect summary by name.
        count = 1 if family == 'clones' and refused else operations
        shape = 4 if family == 'clones' else (1 if refused else 0)
        setup = aggregate.source(count, shape, 1).split('let block :=', 1)[0]
        construct = 'let block := [Codegen materializeResumable: [[source codeObject] withoutSourceProvenance] captures: #[]];'
        call = '[block value: proto value: ' + ('expected' if family == 'clones' else 'observer') + ']'
        expected, identity = 'expected', True
    else:
        # The independent-invocation control creates no consecutive loop stamp.
        setup = ''
        construct = loop_source(operations, 3 if refused else 0, 1).split('let batches :=', 1)[0]
        construct += 'let block := run;'
        call, expected, identity = '[run value: numeric]', '179', False
    operator = '==' if identity else '='
    # Keep fixture carriers in explicit Cells; clearing all three drops the
    # sampler's references. Compiler caches/other runtime roots are independent.
    construct = '[[| ' + construct + f'''
[held set: block];
[invoke set: [|
  let result := {call};
  [[result {operator} {expected}] ifTrue: [| nil] ifFalse: [| [OS exit: 81]]];
  [lastAnswer set: result]
]]; nil ] value];
'''
    run = '[128 timesRepeat: [| [[invoke value] value]]]; [System gc];'
    checks = [('setup', common + setup + '[System gc];'),
              ('cold', construct + '[System gc];'),
              ('warm', run), ('steady-1', run), ('steady-2', run),
              ('evicted-live', '[Codegen evictMaterializations]; [System gc]; ' + run),
              ('released', '[held set: nil]; [invoke set: nil]; [lastAnswer set: nil]; [Codegen evictMaterializations]; [System gc];')]
    return env, [(name, code.replace('\n', ' ') + f' [System println: "memory-checkpoint {name}"];\n') for name, code in checks]


def memory_summary(output):
    # vmmap units are rounded presentation values. Preserve text and original
    # report; never imply byte-exact compiler accounting from these numbers.
    values = {}
    for label in ('Physical footprint', 'Physical footprint (peak)'):
        match = re.search(r'^' + re.escape(label) + r':\s+(\S+)', output, re.M)
        if not match:
            raise RuntimeError('vmmap missing ' + label)
        values[label] = match.group(1)
    if 'MALLOC ZONE' not in output or 'TOTAL' not in output:
        raise RuntimeError('vmmap missing allocator/region totals')
    return values


def send_phase(process, name, code):
    process.stdin.write(code.encode()); process.stdin.flush()
    expected = ('memory-checkpoint ' + name).encode()
    output = bytearray()
    deadline = time.monotonic() + 300
    while True:
        if time.monotonic() >= deadline:
            raise RuntimeError('REPL checkpoint timeout: ' + name)
        if select.select([process.stdout], [], [], 1)[0]:
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError(f'REPL exited before {name}: {bytes(output)!r}')
            output.extend(chunk)
            marker_seen = any(line.removeprefix(b"birnam> ") == expected for line in output.splitlines())
            if marker_seen and output.endswith(b"birnam> "):
                text = output.decode()
                if 'EvaluationRejected' in text or 'error(' in text:
                    raise RuntimeError('REPL evaluation failed: ' + text)
                return text


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', type=Path, required=True)
    p.add_argument('--receipt', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--pairs', type=int, default=3)
    p.add_argument('--families', nargs='+', choices=tuple(SWITCHES), default=list(SWITCHES))
    args = p.parse_args()
    if not 1 <= args.pairs <= 5:
        p.error('pairs must be 1..5')
    prefix = args.prefix.resolve()
    proof = json.loads(args.receipt.read_text())
    if proof.get('status') != 'passed' or proof.get('production_verification') != 'passed' or any(c['exit_code'] for c in proof['commands']):
        p.error('full production verification must pass first')
    if not proof.get('sources') or any(sha(ROOT / n) != h for n, h in proof['sources'].items()):
        p.error('production sources differ from the verified checkpoint')
    sources = dict(proof['sources'])
    sources['scripts/recovery-memory.py'] = sha(__file__)
    manifest_path = prefix / 'share/birnam/rust-install.json'
    manifest = json.loads(manifest_path.read_text())
    if manifest['profile'] != 'release' or manifest['publication']['state'] != 'published':
        p.error('published normal release installation required')
    installed = dict(manifest['staged_manifest'])
    installed['share/birnam/rust-install.json'] = sha(manifest_path)
    shipped = {n.removeprefix('share/birnam/src/'): h for n, h in installed.items() if n.startswith('share/birnam/src/')}
    if not shipped:
        p.error('installed source provenance required')
    sources.update(shipped)
    result = dict(status='running', qualified=False, scope='Whole-process idle memory on the normal release floor, not timing, exact optimizer metadata/code bytes, or proof of consumer activation. Factory bodies reuse prerequisite source counter fixtures. The sampler checks each call result and samples after explicit host checkpoints. Clearing sampler-owned references does not assert that all runtime/compiler roots have disappeared. vmmap sampling may perturb paging and reports rounded platform values.',
                  sources=sources, installed_sha256=installed, correctness_sha256=sha(args.receipt),
                  prefix=str(prefix), pairs=args.pairs, rows=[])
    args.output.parent.mkdir(parents=True, exist_ok=True)

    def save():
        args.output.write_text(json.dumps(result, indent=2) + '\n')

    def current():
        assert all(sha(ROOT / n) == h for n, h in sources.items()), 'source changed'
        assert all(sha(prefix / n) == h for n, h in installed.items()), 'installation changed'
        assert sha(args.receipt) == result['correctness_sha256'], 'proof changed'

    base_env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_', 'BENCH_HOST_'))}
    try:
        current()
        for pair in range(args.pairs):
            for family in dict.fromkeys(args.families):
                for refused in (False, True):
                    env, steps = phases(family, refused)
                    for enabled in ((False, True) if pair % 2 == 0 else (True, False)):
                        current()
                        extra = {SWITCHES[family]: '1'} if enabled else ({SWITCHES[family]: '0'} if family in DEFAULT_ON else {})
                        process = subprocess.Popen([str(prefix / 'bin/birnam'), '--repl'], cwd=ROOT,
                                                   env=dict(base_env, **env, **extra), stdin=subprocess.PIPE,
                                                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                        row = dict(pair=pair, family=family, refused=refused, enabled=enabled, switch_values=extra,
                                   pid=process.pid, phases=[], fixture_sha256=hashlib.sha256(''.join(c for _, c in steps).encode()).hexdigest())
                        result['rows'].append(row);save()
                        try:
                            for name, code in steps:
                                output = send_phase(process, name, code)
                                sample = subprocess.run(['/usr/bin/vmmap', '-summary', str(process.pid)], capture_output=True, text=True, timeout=60)
                                if sample.returncode:
                                    raise RuntimeError('vmmap failed: ' + sample.stderr)
                                row['phases'].append(dict(name=name, input=code, output=output, vmmap=sample.stdout,
                                                          vmmap_stderr=sample.stderr, summary=memory_summary(sample.stdout)))
                                save()
                            process.stdin.close(); process.stdin = None
                            tail, _ = process.communicate(timeout=30)
                            row.update(exit_code=process.returncode, tail=tail.decode())
                            if process.returncode:
                                raise RuntimeError('REPL failed on EOF')
                            print(f'memory-check: pair={pair} family={family} refused={refused} enabled={enabled} passed', flush=True)
                        finally:
                            if process.poll() is None:
                                process.terminate();process.wait(timeout=10)
                        save()
        current()
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error));raise
    finally:
        save()


if __name__ == '__main__':
    main()
