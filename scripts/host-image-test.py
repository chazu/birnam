"""Fresh-process image entry through the installed CLI; source is deleted first."""
import json
import os
from pathlib import Path
import subprocess
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

def snapshot(name, body):
    image = work / (name + '.image')
    source = work / (name + '.birnam')
    source.write_bytes(body + b'; [System saveImage: ' + json.dumps(str(image)).encode() + b']')
    result = run(source, save=True)
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
path = snapshot('missing-main', b'nil')
result = run(path)
assert result.returncode == 0 and b'birnam>' in result.stdout and not result.stderr, result
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
result = run(image, extra=['extra'])
assert result.returncode == 1 and b'one file' in result.stderr, result
# The native Host fixture consumes this exact artifact in a separate process.
print('installed image CLI: saved main, deleted source/assets, nil/false, missing/invalid main and load diagnostics passed')
