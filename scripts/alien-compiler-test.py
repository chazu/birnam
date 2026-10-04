#!/usr/bin/env python3
"""Compare the installed authored compiler with a frozen test-only Rust oracle.

Run outside the checkout with no stdlib or source loading. Besides exact generated
C and diagnostics, check nested/array homogeneous-float ABI metadata.
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
        if actual_kind != kind or actual != expected:
            failures.append((i, cases[i], kind, actual_kind))
    assert not failures, failures
    for i, (signature, expected) in enumerate(abi_cases):
        actual = (work / f'abi{i}').read_text()
        assert actual == expected, (signature, expected, actual)
    print(f'AlienCompiler: {len(cases)} byte-identical C/diagnostic cases; '
          f'{len(abi_cases)} ABI cases; no stdlib or source loading')
