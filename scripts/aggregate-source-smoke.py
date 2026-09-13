#!/usr/bin/env python3
"""Exercise production aggregate admission and late binding through installed Id source."""
import argparse
import hashlib
from itertools import product
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def source(projections, shape, precise):
    statements = ['let copy := [prototype clone];']
    target = 'copy'
    if shape == 2:
        statements.append('let second := [copy clone];')
        target = 'second'
    for index in range(projections):
        if shape == 1 and index == projections - 1:
            statements.append(f'[{target} name]')
        else:
            statements.append(f'let answer := [{target} name];')
    if shape == 3:
        statements.append('let observed := [observer value: copy value: copy];')
    if shape != 1:
        statements.append('answer')
    body = ' '.join(statements)
    if shape == 4:
        body = ' '.join('let copy := [prototype clone];' for _ in range(projections)) + ' observer'
    other = 'expected' if shape == 4 else 'observer'
    setup = f'''[System load: "modules/os.birnam"];
[System preciseGc: {'true' if precise else 'false'}];
let expected := 42;
let proto := [LogicVar named: expected];
let canonicalProto := [proto clone];
let last := [nil asCell];
let effects := [0 asCell];
let observer := [:copy :alias |
  [[copy == proto] ifTrue: [| [OS exit: 41]] ifFalse: [| nil]];
  [[copy == alias] ifTrue: [| nil] ifFalse: [| [OS exit: 42]]];
  [[copy == [last value]] ifTrue: [| [OS exit: 43]] ifFalse: [| nil]];
  [last set: copy]; nil];
let source := [:prototype :observer | {body}];
let block := [Codegen materializeResumable: [[source codeObject] withoutSourceProvenance] captures: #[]];
'''
    phases = []
    for phase in ('native', 'replaced', 'restored', 'canonical'):
        if phase == 'replaced':
            phases.append('[Reflect defineMethod: #name on: proto as: [:self | [effects inc]; expected]];')
        if phase == 'restored':
            phases.append('[Reflect removeMethod: #name on: proto];')
        receiver = 'canonicalProto' if phase == 'canonical' else 'proto'
        phases.append(f'''[System println: "aggregate-phase {phase}"];
[effects set: 0];
[UnboxedNumericProbe run: block input: {receiver} other: {other} expected: 42];
let answer{phase} := [block value: {receiver} value: {other}];
[[answer{phase} == expected] ifTrue: [| nil] ifFalse: [| [OS exit: 44]]];
[[[effects value] = {137 * projections if phase == 'replaced' and shape != 4 else 0}]
 ifTrue: [| nil] ifFalse: [| [OS exit: 45]]];
[System gc];
''')
        if shape == 3:
            phases.append(f'''let escaped{phase} := [last value];
[[escaped{phase} == proto] ifTrue: [| [OS exit: 46]] ifFalse: [| nil]];
[[[escaped{phase} name] == expected] ifTrue: [| nil] ifFalse: [| [OS exit: 47]]];
''')
    post = '''[Codegen evictMaterializations];
[System gc];
let afterEviction := [block value: proto value: observer];
[[afterEviction == expected] ifTrue: [| nil] ifFalse: [| [OS exit: 48]]];
[System println: "aggregate-source identities and current effects passed"];
'''
    if shape == 4:
        post = post.replace('value: observer', 'value: expected')
    # Each complete expression returns to the real host between phases. A
    # removed Id method can retire there; an active outer script would keep
    # that retirement pending and correctly prevent allocation elision.
    return '\n'.join(part.replace('\n', ' ') for part in [setup, *phases, post]) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', required=True, type=Path)
    parser.add_argument('--floor', required=True, type=Path)
    parser.add_argument('--receipt', required=True, type=Path)
    parser.add_argument('--profile', required=True, choices=('debug', 'release'))
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--disabled-consumers', nargs='+', choices=('discarded-clones', 'virtual-fields'), default=[])
    args = parser.parse_args()
    prefix, floor = args.prefix.resolve(), args.floor.resolve()
    proof = json.loads(args.receipt.read_text())
    if proof['status'] != 'passed' or proof.get('production_verification') != 'passed':
        parser.error('full production verification must pass first')
    expected_floor = proof['unboxed_source_matrices'][f'unboxed-measure-{args.profile}.json']['floor_sha256']
    if sha(floor) != expected_floor:
        parser.error('selected floor differs from the verified production checkpoint')
    installed = [prefix / 'bin/birnam', prefix / 'share/birnam/rust-install.json'] + sorted((prefix / 'lib').glob('*.dylib'))
    result = dict(status='running', profile=args.profile, floor_sha256=sha(floor),
                  disabled_consumers=args.disabled_consumers,
                  correctness_sha256=sha(args.receipt), script_sha256=sha(__file__),
                  installed_sha256={str(p): sha(p) for p in installed}, rows=[],
                  scope='Real host REPL, installed compiler and Relational native getter, using the previously verified test floor. Each expression returns to the host between phases so removed code can retire before readmission. Existing numeric probe supplies logical-byte and physical-allocation counters only: its completed/materialized fields count numeric optimization and must remain zero here. Four phases per process: native getter; effectful Id override creating a private vtable; inherited native getter restored with undeclared private layout still refused; unchanged canonical-layout prototype readmitted. Shapes include field-only returns, a tail getter, successive clones, an escaping observer and discarded repeated clones. Selected disabled consumers must omit no physical allocations; unrelated consumers remain active. This is source correctness and admission evidence, not a timing comparison or a proof of every aggregate shape.')

    def check_sources():
        assert all(sha(ROOT / k) == v for k, v in proof['sources'].items()), 'production sources changed since floor verification'
        assert sha(floor) == expected_floor
        assert sha(__file__) == result['script_sha256']
        assert all(sha(k) == v for k, v in result['installed_sha256'].items())

    env = {k: v for k, v in os.environ.items() if not k.startswith(('BIRNAM_', 'DYLD_'))}
    env.update(BIRNAM_ID_DYLIB=str(floor), BIRNAM_TEST_UNBOXED_NUMERIC='1')
    for name in args.disabled_consumers:
        env[{'discarded-clones': 'BIRNAM_DEV_DISCARDED_CLONES',
             'virtual-fields': 'BIRNAM_DEV_VIRTUAL_FIELDS'}[name]] = '0'
    try:
        check_sources()
        with tempfile.TemporaryDirectory(prefix='birnam-aggregate-source-') as work:
            fixture = Path(work) / 'aggregate-repl.txt'
            for projections, shape, precise in product((1, 3, 16), range(5), (0, 1)):
                code = source(projections, shape, precise)
                fixture.write_text(code)
                p = subprocess.run([str(prefix / 'bin/birnam'), '--repl'], input=code, cwd=ROOT, env=env,
                                   text=True, capture_output=True)
                row = dict(projections=projections, shape=shape, precise=precise,
                           source_sha256=sha(fixture), input=code, exit_code=p.returncode, output=p.stdout + p.stderr)
                result['rows'].append(row)
                assert p.returncode == 0, row
                assert 'aggregate-source identities and current effects passed' in p.stdout, row
                phases = []
                measurements = []
                for line in p.stdout.splitlines():
                    if 'aggregate-phase ' in line:
                        phases.append(line.split('aggregate-phase ', 1)[1])
                    if line.startswith('unboxed-numeric '):
                        measurements.append({k: int(v) for k, v in re.findall(r'(\w+)=(-?\d+)', line)})
                assert phases == ['native', 'replaced', 'restored', 'canonical'] and len(measurements) == 4, row
                row['measurements'] = dict(zip(phases, measurements))
                saved = 8 * (2 if shape == 2 else 1) if projections >= 3 and shape in (0, 2) else 0
                if 'virtual-fields' in args.disabled_consumers:
                    saved = 0
                if shape == 4 and projections >= 3 and 'discarded-clones' not in args.disabled_consumers:
                    saved = 8 * projections
                for phase, m in zip(phases, measurements):
                    assert m['invocations'] == 8 and m['answer'] == 42, row
                    assert m['completed'] == m['materialized'] == 0, row
                    assert m['physical_saved'] == (0 if phase in ('replaced', 'restored') else saved), row
                assert measurements[0]['bytes'] == measurements[2]['bytes'] == measurements[3]['bytes'] > 0, row
                print(f'aggregate-source-smoke: {args.profile} projections={projections} shape={shape} precise={precise} passed', flush=True)
        check_sources()
        result['status'] = 'passed'
    except BaseException as error:
        result.update(status='failed', error=str(error))
        raise
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
