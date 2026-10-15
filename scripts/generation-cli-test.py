"""Verify authored lifecycle intents through a published project's console and TCP REPL."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import socket
import time
import sys
import tempfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('prefix', type=Path)
parser.add_argument('--socket-only', action='store_true')
options = parser.parse_args()
binary = options.prefix.resolve() / 'bin' / 'birnam'
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env['BIRNAM_GC_PRECISE'] = '1'
with tempfile.TemporaryDirectory(prefix='birnam-generation-cli-') as temporary:
    root = Path(temporary)
    project = root / 'project'
    (project / 'src').mkdir(parents=True)
    (project / 'birnam.toml').write_text(
        'name = "generation-cli"\nmain = "main.birnam"\n'
        'load = ["generationFixture"]\nsource_roots = ["src"]\n')
    baseline = ('[Module named: #generationFixture requires: #[]]; '
                '[Reflect defineMethod: #generationCliValue on: Integer as: [:self | 10]]')
    (project / 'src/generationFixture.birnam').write_text(baseline)
    (project / 'main.birnam').write_text('true')
    built = subprocess.run([binary, 'build', project], cwd=root, env=env,
                           capture_output=True, timeout=600)
    assert built.returncode == 0, (built.stdout, built.stderr)
    # Entry and the generation baseline must come from the publication.
    (project / 'src/generationFixture.birnam').unlink()
    (project / 'main.birnam').unlink()
    artifact = json.dumps(str(root / 'candidate.o'))
    forms = [
        '[3 generationCliValue]',
        '[[Reflect defineMethod: #generationOrdinaryProbe on: Integer as: [:self | 99]] == Integer]',
        '[[Reflect removeMethod: #generationOrdinaryProbe on: Integer] == Integer]',
        # Published applications own their bundled method definitions as one
        # source revision; replace that admitted revision, preserving its effects.
        f'[CompiledUnit compile: [[Codegen moduleSourceFor: #__generation_cliApplication] replace: "| 10" with: "| 20"] named: #__generation_cliApplication output: {artifact}]',
        f'let consoleCandidate := [CompiledUnit open: {artifact}]',
        '[consoleCandidate map]',
        '[consoleCandidate activate]',
        '[3 generationCliValue]',
        '[consoleCandidate rollback]',
        '[3 generationCliValue]',
        '[consoleCandidate rollback]',
        ':quit',
    ]
    if not options.socket_only:
        result = subprocess.run([binary, 'run', project, '--repl=tty'], cwd=root, env=env,
                                input=('\n'.join(forms) + '\n').encode(),
                                capture_output=True, timeout=900)
        assert result.returncode == 0 and not result.stderr, (result.returncode, result.stdout, result.stderr)
        transcript = result.stdout.decode()
        assert transcript.count('birnam> 10\n') == 2, transcript
        assert transcript.count('birnam> 20\n') == 1, transcript
        assert transcript.index('birnam> 10\n') < transcript.index('birnam> 20\n') < transcript.rindex('birnam> 10\n'), transcript
        assert transcript.count('birnam> true\n') >= 2, transcript
        assert 'error(' not in transcript.lower(), transcript
        print('generation: ordinary running edits, console intent activation and exact rollback passed')

    # Reuse the publication with its source removed, but fresh process state and
    # candidate output. Each reply must be fulfilled before its TCP prompt.
    with socket.socket() as reservation:
        reservation.bind(('127.0.0.1', 0))
        port = reservation.getsockname()[1]
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as error:
        server = subprocess.Popen(
            [binary, 'run', project, '--repl=socket', '--repl-port', str(port),
             '--repl-bind', '127.0.0.1'], cwd=root, env=env, stdout=output, stderr=error)
        try:
            deadline = time.monotonic() + 900
            while True:
                assert server.poll() is None, ('socket exited', server.returncode)
                try:
                    connection = socket.create_connection(('127.0.0.1', port), timeout=1)
                    break
                except OSError:
                    assert time.monotonic() < deadline, 'socket startup timeout'
                    time.sleep(.1)

            def read_prompt(conn):
                reply = b''
                while not reply.endswith(b'birnam> '):
                    chunk = conn.recv(8192)
                    assert chunk, ('socket closed before prompt', reply)
                    reply += chunk
                return reply

            def request(conn, form):
                conn.sendall((form + '\n').encode())
                return read_prompt(conn)

            with connection:
                connection.settimeout(300)
                assert b'birnam socket REPL' in read_prompt(connection)
                replies = []
                for form in forms[:-1]:
                    form = form.replace('candidate.o', 'socket-candidate.o').replace(
                        'consoleCandidate', 'socketCandidate')
                    reply = request(connection, form)
                    assert b'!! ' not in reply and b'error(' not in reply.lower(), reply
                    replies.append(reply)
                assert replies[0] == b'=> 10\r\nbirnam> ', replies
                assert replies[1:3] == [b'=> true\r\nbirnam> '] * 2, replies
                assert replies[7] == b'=> 20\r\nbirnam> ', replies
                assert replies[9] == b'=> 10\r\nbirnam> ', replies
                assert request(connection, 'nil') == b'=> nil\r\nbirnam> '
                assert request(connection, 'false') == b'=> false\r\nbirnam> '
                assert b'!! ' in request(connection, ']')
                assert request(connection, '[1 + 2]') == b'=> 3\r\nbirnam> '
                connection.sendall(b'[1 +\n')
                # Force incomplete input into its own receive turn; no prompt
                # may be emitted until the balancing chunk arrives.
                connection.settimeout(.25)
                try:
                    unexpected = connection.recv(8192)
                    raise AssertionError(('incomplete form replied', unexpected))
                except TimeoutError:
                    pass
                connection.settimeout(300)
                assert request(connection, '2]') == b'=> 3\r\nbirnam> '
                assert request(connection, 'let socketPersistent := 41') == b'=> 41\r\nbirnam> '
                connection.sendall(b'[unfinished\n')
                connection.shutdown(socket.SHUT_WR)
                assert connection.recv(8192) == b''
            # EOF discards the first client's partial form but retains globals.
            with socket.create_connection(('127.0.0.1', port), timeout=30) as again:
                again.settimeout(300)
                read_prompt(again)
                assert request(again, '[socketPersistent + 1]') == b'=> 42\r\nbirnam> '
                assert request(again, '[3 generationCliValue]') == b'=> 10\r\nbirnam> '
                again.sendall(b':quit\n')
                assert again.recv(8192) == b''
            with socket.create_connection(('127.0.0.1', port), timeout=30) as final:
                final.settimeout(300)
                read_prompt(final)
                assert request(final, '[2 + 2]') == b'=> 4\r\nbirnam> '
                final.sendall(b':quit\n')
                assert final.recv(8192) == b''
        finally:
            server.terminate()
            server.wait(timeout=15)
        error.seek(0)
        assert not error.read(), 'socket wrote diagnostics to stderr'
    print('generation: TCP activation/rollback, ordinary edits, multiline, diagnostics and successive sessions passed')
