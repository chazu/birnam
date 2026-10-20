#!/usr/bin/env python3
"""Frozen ImageFormat bytes, rejection rules and request-local error isolation."""
import json
import os
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent.parent
prefix = Path(sys.argv[1]).resolve()
# Optional compiled prototype for development; production tests load only bootstrap.
artifact = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else None
receiver = 'ImageFormatBirnam' if artifact else 'ImageFormat'
target = root / '.cargo-target/image-format-reference'
subprocess.run(['cargo', 'build', '--manifest-path', str(root / 'tests/reference/image-format/Cargo.toml'),
                '--offline', '--locked', '--target-dir', str(target)], check=True)
fixture = root / 'tests/fixtures/portable-code-v1'
images = sorted(fixture.glob('image-*.bin')) + sorted((root / 'tests/fixtures/stackless-s2').glob('image-*.bin'))
empty = (fixture / 'image-empty-stripped.bin').read_bytes()
mixed = (fixture / 'image-mixed-facts-stripped.bin').read_bytes()
cases = [p.read_bytes() for p in images]
# Representative framing boundaries, corrupt records and forged unsigned lengths.
cases += [mixed[:n] for n in [0, 1, 7, 8, 12, 20, 27, 28, 39, 40, 103, 104, 155, 256, len(mixed)-1]]
rng = random.Random(8)
for index in rng.sample(range(len(mixed)), 64):
    b = bytearray(mixed); b[index] ^= 0xff; cases.append(bytes(b))
for offset in [32, 140, 28 + 76 + 36]:
    for value in [0, 1, 2**63 - 1, 2**63, 2**64 - 1]:
        b = bytearray(mixed); b[offset:offset+8] = struct.pack('<Q', value); cases.append(bytes(b))
def record(kind, payload):
    return struct.pack('<IQ', kind, len(payload)) + payload
def export(name):
    b = name.encode(); return b'\x09' + struct.pack('<I', len(b)) + b
def obj(name, slots):
    return export(name) + struct.pack('<I', len(slots)) + b''.join(slots)
# Private behavior family, owner, slot-layout and cycle rules.
objects = record(3, struct.pack('<I', 2) + obj('ObjectVt', []) + obj('ObjectVt', []))
for links in [
    struct.pack('<III', 1, 0, 0) + b'\x00' + struct.pack('<I', 8) + b'ObjectVt',
    struct.pack('<III', 1, 0, 0) + b'\x01' + struct.pack('<I', 0),
    struct.pack('<III', 1, 1, 0),
    struct.pack('<III', 1, 3, 3) + b'\x00' + struct.pack('<I', 8) + b'ObjectVt',
    struct.pack('<III', 1, 0, 0) + b'\x00' + struct.pack('<I', 8) + b'StringVt',
]:
    cases.append(empty + record(11, links) + objects)
cases += [empty + objects, empty + record(3, struct.pack('<I', 1) + obj('SystemVt', [b'\x00'])),
          empty + record(3, struct.pack('<I', 1) + obj('ObjectVt', [b'\x07' + struct.pack('<I', 99)])),
          empty + b'\x00', empty + record(4, b'')]
# Facts use exact unsigned u64 ordering, including adjacent values which
# ordinary Integer ordered comparisons round to the same f64 value.
for basis in [2**53, 2**63 - 1, 2**63, 2**63 + 42, 2**64 - 2]:
    for floor, policy, argument in [(basis+1, 0, 0), (basis, 1, basis+1), (basis-1, 0, 0)]:
        payload = struct.pack('<QQB7xQI', basis, floor, policy, argument, 0)
        cases.append(empty + record(2, payload))
# Provider declarations: empty/core, valid optional closure, duplicates, bad
# types/counts/framing, and out-of-order or repeated records.
def symbol(name):
    b=name.encode();return b'\x05'+struct.pack('<I',len(b))+b
for payload in [struct.pack('<I',0), struct.pack('<I',2)+symbol('store')+symbol('relational'),
                struct.pack('<I',2)+symbol('store')*2,struct.pack('<I',1)+export('Store'),
                b'',struct.pack('<I',4097),struct.pack('<I',1),struct.pack('<I',0)+b'x']:
    cases.append(empty+record(12,payload))
providers=record(12,struct.pack('<I',0))
cases += [empty+providers+providers,empty+objects+providers]
with tempfile.TemporaryDirectory(prefix='birnam-image-format-') as temporary:
    work = Path(temporary)
    paths = []
    for i, b in enumerate(cases):
        p = work / f'{i}.image'; p.write_bytes(b); paths.append(p)
    expected = subprocess.run([target / 'debug/birnam-image-format-reference'],
        input=''.join(str(p)+'\n' for p in paths).encode(), capture_output=True, check=True).stdout.decode().splitlines()
    load = f'[Codegen load: {json.dumps(str(artifact))}];' if artifact else ''
    forms = [load, f'let paths := #[{" ".join(json.dumps(str(p)) for p in paths)}];',
             f'[{receiver} each: paths do: [:path :i |',
             '  [System println: i];',
             '  let bytes := [[System readFile: path] asBytes];',
             f'  let answer := [{receiver} inspect: bytes architecture: 1 abi: -3623637935742366395];',
             '  [System println: [[answer isError] ifTrue: [| [[[answer kind] == #imageRejected] ifTrue: [| "err"] ifFalse: [| "wrongError"]]] ifFalse: [| "ok"]]];',
             '  [[answer isError] ifFalse: [|',
             '    let all := [Cell with: nil];',
             '    [all set: #[1 [bytes readU64At: 20]]];',
             f'    [{receiver} each: answer do: [:field :j |',
             '      let value := [[[j = 5] & [field notNil]] ifTrue: [| [field copyFrom: 72 to: [field size]]] ifFalse: [| field]];',
             '      [all set: [[all value] add: value]]]];',
             f'    let encoded := [{receiver} compose: [all value]];',
             '    [System writeFile: [path , ".out"] contents: [encoded asString]]',
             '  ]]', ']];']
    # Business validation must preserve its refusal, not merely return any Error.
    error_probe = work / 'error-contract.birnam'
    error_probe.write_text(load + f'''
let answer := [{receiver} inspect: ["short" asBytes] architecture: 1 abi: 0];
[System println: [answer kind]];
[System println: [answer message]]
''')
    probe = subprocess.run([prefix/'bin/birnam', error_probe], cwd=work,
        env={**{k:v for k,v in os.environ.items() if not k.startswith('BIRNAM_')},
             'BIRNAM_NO_STDLIB':'1'}, capture_output=True, timeout=30)
    assert probe.returncode == 0 and probe.stdout == b'#imageRejected\ninspectBytes:architecture:abi:\n' and not probe.stderr, probe
    # Use the fixed ABI bit pattern as an Integer, matching the native boundary.
    abi = struct.unpack('<q', struct.pack('<Q', 0xcdb641f978540d45))[0]
    text = '\n'.join(forms).replace('-3623637935742366395', str(abi))
    script = work / 'proof.birnam'; script.write_text(text)
    env = {k:v for k,v in os.environ.items() if not k.startswith('BIRNAM_')}
    env['BIRNAM_NO_STDLIB'] = '1'
    try:
        r = subprocess.run([prefix/'bin/birnam', script], cwd=work, env=env, capture_output=True, timeout=120)
    except subprocess.TimeoutExpired as e:
        print('Timed out after output:', (e.stdout or b'').decode(), flush=True)
        raise
    assert r.returncode == 0 and not r.stderr, (r.returncode, r.stdout, r.stderr)
    actual = r.stdout.decode().splitlines()[1::2]
    assert len(actual) == len(expected), (len(actual), len(expected), actual)
    for i, (got, want) in enumerate(zip(actual, expected)):
        parts = want.split('\t')
        assert got == parts[0], (i, got, parts[0])
        if got == 'ok':
            assert (work / f'{i}.image.out').read_bytes() == bytes.fromhex(parts[1]), i
    print(f'ImageFormat: {len(cases)} admission cases; {actual.count("ok")} byte-identical roundtrips; no stdlib/source fallback')
