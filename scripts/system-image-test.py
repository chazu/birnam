"""Live OS services after source-free Image recovery and baked entry."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

prefix = Path(sys.argv[1]).resolve()
binary = prefix / 'bin/birnam'
env = {k: v for k, v in os.environb.items() if not k.startswith(b'BIRNAM_')}
# Bytes preserve the deliberately invalid UTF-8 environment value.
env.update({b'BIRNAM_GC_PRECISE': b'1', b'IMAGE_VALUE': b'saved environment',
            b'IMAGE_EMPTY': b'', b'IMAGE_BAD': b'\xff'})
env.pop(b'IMAGE_MISSING', None)
with tempfile.TemporaryDirectory(prefix='birnam-system-image-') as temporary:
    work = Path(temporary)
    quote = lambda p: json.dumps(str(p))
    incoming, outgoing = work / 'input', work / 'output'
    incoming.write_bytes(b'a\0\xff\r\n')
    image = work / 'services.image'
    source = work / 'save.birnam'
    setup = f'''
let receiver := [System clone]; let alias := receiver;
let payload := [receiver readFile: {quote(incoming)}]; let payloadAlias := payload;
let savedEnvironment := [receiver getEnv: "IMAGE_VALUE"];
let savedInput := [receiver readLine];
'''
    checks = [
        ('[receiver == alias]', b'true'),
        ('[payload == payloadAlias]', b'true'),
        ('[[receiver autoGc: false] == receiver]', b'true'),
        ('[[receiver preciseGc: true] == receiver]', b'true'),
        ('[[receiver gc] >= 0]', b'true'),
        ('[[receiver liveObjects] > 0]', b'true'),
        ('[[receiver autoGc: nil] isNil]', b'true'),
        ('[[receiver preciseGc: 0] isNil]', b'true'),
        ('savedInput', b'saved input'),
        ('savedEnvironment', b'saved environment'),
        ('[receiver getEnv: "IMAGE_VALUE"]', b'live environment'),
        ('[[receiver getEnv: "IMAGE_EMPTY"] = ""]', b'true'),
        ('[[receiver getEnv: "IMAGE_MISSING"] isNil]', b'true'),
        ('[[receiver getEnv: "IMAGE_BAD"] kind]', b'#ioError'),
        ('[[receiver getEnv: "IMAGE_BAD"] message]', b'could not read environment'),
        (f'[[receiver writeFile: {quote(outgoing)} contents: payload] == receiver]', b'true'),
        (f'[[receiver readFile: {quote(outgoing)}] = payload]', b'true'),
        (f'[receiver readFile: {quote(incoming)}]', b'live file'),
        (f'[[receiver readFile: {quote(work / "missing")}] kind]', b'#ioError'),
        (f'[[receiver writeFile: {quote(work)} contents: payload] message]', b'could not write file'),
        ('[receiver readLine]', b'a\0\xff\r'),
        ('[receiver readLine]', b'last'),
        ('[[receiver readLine] isNil]', b'true'),
        ('[[receiver preciseGc: false] == receiver]', b'true'),
        ('[[receiver autoGc: true] == receiver]', b'true'),
    ]
    body = '\n'.join('[System println: ' + expression + '];' for expression, _ in checks)
    source.write_text(setup + 'let main := [|\n' + body + '\n];\n'
                      + f'[System saveImage: {quote(image)}]; [System println: [[System imageSaveReport] status]]')
    def run(args, data=b'', environment=env, stdin=None):
        kwargs = dict(stdin=stdin) if stdin is not None else dict(input=data)
        result = subprocess.run(args, cwd=work, env=environment, capture_output=True,
                                timeout=300, **kwargs)
        assert result.returncode == 0 and not result.stderr, (args, result)
        return result.stdout
    assert run([binary, source], b'saved input\n') == b'#saved\n'
    assert image.is_file()
    source.unlink()
    incoming.write_bytes(b'live file')
    live = env.copy()
    live[b'IMAGE_VALUE'] = b'live environment'
    expected = b''.join(value + b'\n' for _, value in checks)
    shutil.copytree(prefix, work / 'bundle')
    baked = work / 'bundle/bin/app'
    assert run([binary, '--bake', image, baked]) == b''
    for args in ([binary, image], [binary, '--rebuild', image], [baked]):
        actual = run(args, b'a\0\xff\r\nlast', live)
        assert actual == expected, (args, actual, expected)
        assert outgoing.read_bytes() == b'a\0\xff\r\n'
    # A directory fd produces a real read failure, not EOF, in each fresh process.
    errors = work / 'input-error.image'
    source.write_text('let main := [| let error := [System readLine]; '
                      '[System println: [error kind]]; [System println: [error message]]]; '
                      f'[System saveImage: {quote(errors)}]')
    assert run([binary, source]) == b'' and errors.is_file()
    source.unlink()
    baked = work / 'bundle/bin/input-error'
    assert run([binary, '--bake', errors, baked]) == b''
    for args in ([binary, errors], [binary, '--rebuild', errors], [baked]):
        fd = os.open(work, os.O_RDONLY)
        try:
            actual = run(args, stdin=fd)
            assert actual == b'#ioError\nstdin read failed\n', (args, actual)
        finally:
            os.close(fd)
print('Image OS services: aliases, collector controls, binary input/output, EOF/errors and live file/environment state passed')
