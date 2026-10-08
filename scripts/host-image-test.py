"""Fresh-process image entry through the installed CLI; source is deleted first."""
import json
import os
from pathlib import Path
import subprocess
import struct
import sys

prefix = Path(sys.argv[1]).resolve()
work = prefix.parent / 'image checks'
work.mkdir()
binary = prefix / 'bin/birnam'
environment = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
environment['BIRNAM_GC_PRECISE'] = '1'

def run(path, *, save=False, extra=()):
    env = environment.copy()
    if save:
        env['BIRNAM_NO_STDLIB'] = '1'
    return subprocess.run([binary, path, *extra], cwd=work, env=env,
                          input=b":quit\n", stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=300)

def snapshot(name, body, *, stdlib=False):
    image = work / (name + '.image')
    source = work / (name + '.birnam')
    source.write_bytes(body + b'; [System saveImage: ' + json.dumps(str(image)).encode() + b']')
    result = run(source, save=not stdlib)
    assert result.returncode == 0 and not result.stderr and image.is_file(), result
    source.unlink()
    return image

# Regression: inherited source methods live directly on exported behaviors.
inherited = snapshot('inherited', b'[Reflect defineMethod: #collect: on: Collection as: [:self :blk | [self inject: #[] into: [:acc :e | [acc add: [blk value: e]]]]]]; '
                     b'[Globals at: #main put: [| [System println: [#[1 2 3] collect: [:x | [x + 1]]]]]]')
result = run(inherited)
assert result.returncode == 0 and result.stdout == b'#[2 3 4]\n' and not result.stderr, result

(work / 'asset').write_bytes(b'embedded image data')
image = snapshot('main', b'[Globals at: #main put: [| [System gc]; [System println: [[System embedFile: "asset"] size]]; nil]]')
(work / 'asset').unlink()
result = run(image)
assert result.returncode == 0 and result.stdout == b'19\n' and not result.stderr, result
for value in [b'nil', b'false']:
    path = snapshot(value.decode(), b'[Globals at: #main put: [| ' + value + b']]')
    result = run(path)
    assert result.returncode == 0 and not result.stdout and not result.stderr, result
path = snapshot('missing-main', b'nil', stdlib=True)
result = run(path)
assert result.returncode == 0 and b'birnam>' in result.stdout and not result.stderr, result
path = snapshot('native-only-no-main', b'[Globals at: #preserved put: 42]')
result = run(path)
assert result.returncode == 1 and b'image lacks an authored bootstrap' in result.stderr, result
recovery_env = environment.copy()
recovery_env['BIRNAM_FORCE_SOURCE'] = '1'
result = subprocess.run([binary, path], cwd=work, env=recovery_env,
                        input=b'preserved\n:quit\n', capture_output=True, timeout=300)
assert result.returncode == 0 and b'birnam> 42' in result.stdout and not result.stderr, result
for name, body, diagnostic in [
    ('invalid-main', b'[Globals at: #main put: 42]', b'image main:'),
    ('wrong-arity', b'[Globals at: #main put: [:arg | arg]]', b'image main:'),
    ('error', b'[Globals at: #main put: [| [System readFile: "/absent/birnam-image-fixture"]]]', b'could not read file'),
]:
    path = snapshot(name, body)
    result = run(path)
    assert result.returncode == 1 and not result.stdout and diagnostic in result.stderr, (name, result)
    assert os.fsencode(path) in result.stderr, result
bad = work / 'corrupt.image'
bad.write_bytes(b'not an image')
for path, diagnostic in [(bad, b'image load #rejected'), (work / 'absent.image', b'No such file')]:
    result = run(path)
    assert result.returncode == 1 and diagnostic in result.stderr and not result.stdout, result
# The current format is exact: reject prior/future versions and invalid magic
# before any saved entry can run. These mutations preserve the rest of a real save.
for version in (4, 5, 6, 7, 9):
    changed = bytearray(image.read_bytes())
    struct.pack_into('<I', changed, 8, version)
    rejected = work / f'version-{version}.image'
    rejected.write_bytes(changed)
    result = run(rejected)
    assert result.returncode == 1 and b'image load #rejected' in result.stderr and not result.stdout, result
changed = bytearray(image.read_bytes())
changed[0] ^= 1
rejected = work / 'invalid-magic.image'
rejected.write_bytes(changed)
result = run(rejected)
assert result.returncode == 1 and b'image load #rejected' in result.stderr and not result.stdout, result
result = run(image, extra=['extra'])
assert result.returncode == 1 and b'one file' in result.stderr, result
# The native Host fixture consumes this exact artifact in a separate process.
print('installed image CLI: saved main, deleted source/assets, nil/false, missing/invalid main and load diagnostics passed')
