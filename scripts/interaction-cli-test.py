"""Exercise installed interactive modes, image tools and scaffolding from another cwd."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

prefix = Path(sys.argv[1]).resolve()
binary = prefix / 'bin/birnam'
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env['BIRNAM_GC_PRECISE'] = '1'
with tempfile.TemporaryDirectory(prefix='birnam-interaction-') as directory:
    work = Path(directory)

    def run(args, data=b'', cwd=work, ok=True, timeout=600):
        p = subprocess.run([binary, *map(str, args)], cwd=cwd, env=env,
                           input=data, capture_output=True, timeout=timeout)
        assert p.returncode == (0 if ok else 1), (args, p.returncode, p.stdout, p.stderr)
        if ok:
            assert not p.stderr, (args, p.stderr)
        return p

    run(['init'])
    config = (work / 'birnam.toml').read_bytes()
    run(['init'], ok=False)
    assert (work / 'birnam.toml').read_bytes() == config
    run(['wrap', 'sample'])
    wrapper = work / 'wrapped/sample/sample.birnam'
    assert '#sample' in wrapper.read_text()
    wrapper.write_text('// preserved\n')
    run(['wrap', 'sample'])
    assert wrapper.read_text() == '// preserved\n'
    assert b'String' in run(['doc', 'stdlib']).stdout
    html = work / 'reference.html'
    run(['docgen', html], timeout=int(os.environ.get('BIRNAM_DOCGEN_TEST_TIMEOUT', '1800')))
    assert '<html' in html.read_text().lower() and 'println:' in html.read_text()
    print('tooling: init/wrap preservation, doc and docgen passed', flush=True)

    # Baked entry must recover its main after the source and image are deleted.
    source = work / 'bake.birnam'
    image = work / 'saved.image'
    source.write_text('[Globals at: #main put: [| [System println: "baked-main"]]]; '
                      '[System saveImage: ' + json.dumps(str(image)) + ']')
    run([source])
    assert image.is_file()
    assert run(['--rebuild', image]).stdout == b'baked-main\n'
    baked = prefix / 'bin/interaction-baked'
    try:
        run(['--bake', image, baked])
        image.unlink()
        source.unlink()
        p = subprocess.run([baked], cwd=work, env=env, capture_output=True, timeout=120)
        assert p.returncode == 0 and p.stdout == b'baked-main\n' and not p.stderr, p
    finally:
        baked.unlink(missing_ok=True)
    print('image tooling: rebuild and executable trailer recovery passed', flush=True)

    project = work / 'project'
    (project / 'src').mkdir(parents=True)
    manifest = project / 'birnam.toml'
    manifest.write_text('name = "interactive"\nmain = "main.birnam"\n[run]\nrepl = "tty"\n')
    (project / 'main.birnam').write_text('[System println: "main-once"]; let sessionValue := 40')
    run(['build', project])
    # The caller's config must never run during published project entry.
    (work / 'birnam.toml').write_text('load = ["callerMustNotLoad"]\n')
    manifest.write_text('invalid edited manifest')
    (project / 'main.birnam').unlink()
    result = run(['run', project], b'[sessionValue + 2]\n:quit\n')
    assert result.stdout.count(b'main-once') == 1 and b'birnam> 42' in result.stdout, result
    assert run(['run', project, '--no-repl']).stdout == b'main-once\n'

    def socket_session(args, expression=b'let socketValue := 40\n', expected=b'=> 40', prepare=None):
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0))
            port = reservation.getsockname()[1]
        if prepare:
            prepare(port)
        args = [str(port) if arg == '{port}' else str(arg) for arg in args]
        with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as error:
            process = subprocess.Popen([binary, *args], cwd=work, env=env, stdout=output, stderr=error)
            try:
                deadline = time.monotonic() + 900
                while True:
                    assert process.poll() is None, ('socket exited', process.returncode)
                    try:
                        connection = socket.create_connection(('127.0.0.1', port), timeout=1)
                        break
                    except OSError:
                        assert time.monotonic() < deadline, 'socket startup timeout'
                        time.sleep(.1)
                with connection:
                    connection.settimeout(30)
                    def read_prompt():
                        result = b''
                        while not result.endswith(b'birnam> '):
                            chunk = connection.recv(8192)
                            assert chunk, result
                            result += chunk
                        return result
                    assert b'birnam' in read_prompt()
                    connection.sendall(b'[[Reflect defineMethod: #socketGuardProbe on: Integer as: [:self | 99]] == Integer]\n')
                    assert b'=> true' in read_prompt()
                    connection.sendall(expression)
                    assert expected in read_prompt()
                    connection.sendall(b'nil\n')
                    assert b'=> nil' in read_prompt()
                    connection.sendall(b':quit\n')
            finally:
                process.terminate()
                process.wait(timeout=15)
            output.seek(0)
            error.seek(0)
            assert not error.read()
            return output.read()

    socket_session(['--listen', '{port}'])
    result = socket_session(['run', project, '--repl=socket', '--repl-port', '{port}',
                             '--repl-bind', '127.0.0.1'], b'[sessionValue + 2]\n', b'=> 42')
    assert result.count(b'main-once') == 1, result
    def publish_socket(port):
        manifest.write_text('name = "interactive"\nmain = "main.birnam"\n[run]\n'
                            'repl = "socket"\nsocket_bind = "127.0.0.1"\n'
                            f'socket_port = {port}\n')
        (project / 'main.birnam').write_text('[System println: "main-once"]; let sessionValue := 40')
        run(['build', project])
        manifest.write_text('invalid edited manifest')
        (project / 'main.birnam').unlink()
    result = socket_session(['run', project], b'[sessionValue + 2]\n', b'=> 42', publish_socket)
    assert result.count(b'main-once') == 1, result
    with socket.socket() as occupied:
        occupied.bind(('0.0.0.0', 0))
        occupied.listen()
        run(['--listen', occupied.getsockname()[1]], ok=False)
    print('interactive: persistent TCP sessions, bind failure, published project tty/socket and main-once passed', flush=True)
