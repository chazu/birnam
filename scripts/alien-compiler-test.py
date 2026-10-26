#!/usr/bin/env python3
"""Compare the installed authored compiler with a frozen test-only Rust oracle.

Run outside the checkout with no stdlib or source loading. Compare the unchanged
foreign marshalling and diagnostics exactly; exercise the Oop adapter through
independently compiled C functions and callbacks.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent.parent
prefix = Path(sys.argv[1]).resolve()
binary = prefix / 'bin/birnam'
assert not (prefix / 'lib/libbirnam_alien_compiler.dylib').exists()
assert (prefix / 'lib/libbirnam_alienCompiler.dylib').read_bytes().startswith(b'\x7fELF')
for asset in (root / 'modules/alien-compiler-assets').iterdir():
    bundled = prefix / 'share/birnam/src/modules/alien-compiler-assets' / asset.name
    assert bundled.read_bytes() == asset.read_bytes(), asset.name
reference_target = root / '.cargo-target/alien-reference'
subprocess.run([
    'cargo', 'build', '--manifest-path',
    str(root / 'tests/reference/alien-compiler/Cargo.toml'),
    '--offline', '--locked', '--target-dir', str(reference_target),
], check=True)
valid = [
    'l l', 'i i', 'u u', 'd d', 'f f', 's s', 'p p', 'v', 'l l l',
    '{ll} l', 'l {ll}', '{dd} d', '{fff} {fff}', '{l3i} {l3i}',
    '{{ll}{ll}}{l3i}', '{2p} {2p}', '{fd} {fd}', '{5f} {5f}',
    '{d{dd}} {d{dd}}',
]
cases = [('sample', s) for s in valid] + [(None, s) for s in valid]
cases += [('sample', s) for s in [
    '', ' ', '\t', 'x', '{', '{}', '{s}', '{v}', 'iv', '{0i}', '{3s}',
    '{3}', '{9999999999999999999999i}', '{18446744073709551616i}', 'i\n l',
]]
cases += [('', 'l l'), ('name;bad', 'l l'), ('9bad', 'l l')]
cases += [('sample', s) for s in [
    '{0003i} {0003i}', '{18446744073709551615i}', '{0}', '{123', 'l{',
    'i\r', '{i\t l}', '{s\t}',
]]
request = ''.join(
    ('-' if name is None else name.encode().hex()) + '\t' + sig.encode().hex() + '\n'
    for name, sig in cases
)
reference = subprocess.run(
    [reference_target / 'debug/birnam-alien-compiler-reference'],
    input=request.encode(), capture_output=True, check=True,
).stdout.decode().splitlines()
old_prelude = (root / 'tests/reference/alien-compiler/src/native_prelude.c').read_bytes()
oop_prelude = (root / 'modules/alien-compiler-assets/native-prelude.c').read_bytes()

def foreign_code(source, prelude, callback):
    assert source.startswith(prelude), 'unexpected generated prelude'
    source = source[len(prelude):]
    if prelude == old_prelude:
        # The old adapter stored an Outcome; the Oop adapter stores the failing
        # reference itself. Preserve the guard while comparing its foreign body.
        source = source.replace(b'active->failure.status', b'active->failure')
    if callback:
        # emit_native renamed the foreign body to __cb_body, then appended a
        # version-specific rooted callback entry. Keep the entire foreign body.
        entry = source.index(b' __cb(')
        return source[:source.rfind(b'\n', 0, entry) + 1]
    marker = b'\nstatic O invoke(' if prelude == oop_prelude else b'\nstatic R invoke('
    return source[:source.index(marker)]
abi_cases = [
    ('{dd}', '#[d 2]'), ('{4f}', '#[f 4]'),
    ('{d{dd}}', '#[d 3]'), ('{{2f}{2f}}', '#[f 4]'),
    ('{fd}', 'nil'), ('{5f}', 'nil'), ('{l3i}', 'nil'),
    ('{18446744073709551615f}', 'nil'),
]
with tempfile.TemporaryDirectory(prefix='birnam-alien-equivalence-') as temporary:
    work = Path(temporary)
    script = work / 'proof.birnam'
    forms = []
    for i, (name, signature) in enumerate(cases):
        output, kind = work / f'{i}.result', work / f'{i}.kind'
        name_literal = 'nil' if name is None else json.dumps(name)
        forms.extend([
            f'let answer := [AlienCompiler compile: {name_literal} signature: {json.dumps(signature)}];',
            f'[System writeFile: {json.dumps(str(kind))} contents: [[answer isError] ifTrue: [| "err"] ifFalse: [| "ok"]]];',
            f'[System writeFile: {json.dumps(str(output))} contents: [[answer isError] ifTrue: [| [answer message]] ifFalse: [| answer]]];',
        ])
    for i, (signature, _) in enumerate(abi_cases):
        forms.append(
            f'[System writeFile: {json.dumps(str(work / f"abi{i}"))} contents: '
            f'[[[[AlienCompiler classify: [AlienCompiler parse: {json.dumps(signature)}]] at: 0] at: 2] printString]];'
        )
    script.write_text('\n'.join(forms))
    env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
    env['BIRNAM_NO_STDLIB'] = '1'
    result = subprocess.run([binary, script], cwd=work, env=env,
                            capture_output=True, timeout=90)
    assert result.returncode == 0 and not result.stderr, (
        result.returncode, result.stdout, result.stderr,
    )
    failures = []
    for i, line in enumerate(reference):
        kind, encoded = line.split('\t')
        expected = bytes.fromhex(encoded)
        actual = (work / f'{i}.result').read_bytes()
        actual_kind = (work / f'{i}.kind').read_text()
        matches = actual == expected
        if kind == actual_kind == 'ok':
            matches = foreign_code(actual, oop_prelude, cases[i][0] is None) == foreign_code(expected, old_prelude, cases[i][0] is None)
        if actual_kind != kind or not matches:
            failures.append((i, cases[i], kind, actual_kind))
    assert not failures, failures
    for i, (signature, expected) in enumerate(abi_cases):
        actual = (work / f'abi{i}').read_text()
        assert actual == expected, (signature, expected, actual)
    print(f'AlienCompiler: {len(cases)} byte-identical foreign-marshalling/diagnostic cases; '
          f'{len(abi_cases)} ABI cases; no stdlib or source loading')

    library = work / 'foreign.dylib'
    subprocess.run(['clang', '-dynamiclib', '-Wall', '-Wextra', '-Werror',
                    root / 'scripts/fixtures/alien-compiler-abi.c', '-o', library], check=True)
    bindings = [
        ('ac_long', 'l l', 'acLong', 'Integer'), ('ac_int', 'i i', 'acInt', 'Integer'),
        ('ac_unsigned', 'u u', 'acUnsigned', 'Word'), ('ac_double', 'd d', 'acDouble', 'Float'),
        ('ac_float', 'f f', 'acFloat', 'Float'), ('ac_string', 's s', 'acString', 'String'),
        ('ac_pointer', 'p p', 'acPointer', 'ForeignAddress'), ('ac_store', 'v l', 'acStore', 'Integer'),
        ('ac_stored', 'l', 'acStored', 'Integer'), ('ac_pair', '{dd} {dd}', 'acPair', 'Vector'),
        ('ac_four', '{4f} {4f}', 'acFour', 'Vector'), ('ac_five', '{5f} {5f}', 'acFive', 'Vector'),
        ('ac_nested', '{d{dd}} {d{dd}}', 'acNested', 'Vector'), ('ac_mixed', '{l3i} {l3i}', 'acMixed', 'Vector'),
        ('ac_call_long', 'l p l', 'acCallLong:', 'ForeignAddress'),
        ('ac_call_pair', '{dd} p {dd}', 'acCallPair:', 'ForeignAddress'),
        ('ac_call_nested', '{d{dd}} p {d{dd}}', 'acCallNested:', 'ForeignAddress'),
    ]
    forms = [f'[Alien dlopen: {json.dumps(str(library))}];']
    forms += [f'[Alien wrap: "{name}" signature: "{signature}" as: #{selector} on: {target}];'
              for name, signature, selector, target in bindings]
    forms += ['let pointer := [Alien alloc: 8];', '[41 acStore];']
    def aggregate_check(expression, expected):
        def fields(receiver, value):
            if not isinstance(value, list):
                return [f'[{receiver} = {value}]']
            checks = [f'[[{receiver} size] = {len(value)}]']
            for index, child in enumerate(value):
                checks += fields(f'[{receiver} at: {index}]', child)
            return checks
        terms = fields('result', expected)
        answer = terms[0]
        for term in terms[1:]: answer = f'[{answer} & {term}]'
        return f'[[| let result := {expression}; {answer}] value]'

    checks = [
        '[[-7 acLong] = -4]', '[[7 acInt] = 4]',
        '[[[Execution wordResultOf: -1 selector: #word] acUnsigned] = [Execution wordResultOf: -1 selector: #word]]',
        '[[1.25 acDouble] = 1.75]', '[[1.25 acFloat] = 1.75]',
        '[["copied" acString] = "copied"]', '[[pointer acPointer] = pointer]',
        '[[0 acStored] = 41]',
        aggregate_check('[#[1.25 2.5] acPair]', [2.25, 4.5]),
        aggregate_check('[#[#[1.0 2.0 3.0 4.0]] acFour]', [[1.0, 2.0, 3.0, 5.0]]),
        aggregate_check('[#[#[1.0 2.0 3.0 4.0 5.0]] acFive]', [[1.0, 2.0, 3.0, 4.0, 6.0]]),
        '[[#[1.0 2.0 3.0 4.0] acFour] isNil]',
        aggregate_check('[#[1.0 #[2.0 3.0]] acNested]', [1.0, [2.0, 4.0]]),
        aggregate_check('[#[7 #[1 2 3]] acMixed]', [8, [1, 2, 5]]),
    ]
    forms += [f'[System println: {check}];' for check in checks]
    forms += ['[pointer free];']
    for signature, selector, value, expected in [('l l', 'acCallLong:', '41', 41),
                                      ('{dd} {dd}', 'acCallPair:', '#[1.25 2.5]', [1.25, 2.5]),
                                      ('{d{dd}} {d{dd}}', 'acCallNested:', '#[1.0 #[2.0 3.0]]', [1.0, [2.0, 3.0]])]:
        check = aggregate_check(f'[callback {selector} {value}]', expected)
        forms += [f'let callback := [Alien callback: [:value | [System gc]; value] signature: "{signature}"];',
                  f'[System println: {check}];',
                  '[Alien releaseCallback: callback];', '[System println: [callback isNull]];']
    script.write_text('\n'.join(forms))
    result = subprocess.run([binary, script], cwd=work, env=env, capture_output=True, timeout=180)
    assert result.returncode == 0 and not result.stderr and result.stdout == b'true\n' * (len(checks) + 6), result
    print(f'AlienCompiler: {len(checks) + 6} real Oop adapter checks; scalar/aggregate calls and collecting callbacks passed')
