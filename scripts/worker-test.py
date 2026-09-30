"""Exercise the installed CLI and fresh image workers over the real wire channel."""
import json
import os
from pathlib import Path
import socket
import shlex
import time
import struct
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve() / 'bin/birnam'
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env.update(BIRNAM_NO_STDLIB='1', BIRNAM_GC_PRECISE='1')
with tempfile.TemporaryDirectory(prefix='birnam-workers-') as temporary:
    work = Path(temporary)
    image = work / 'worker.image'
    def run(name, source, expected=b'', *, stdlib=False):
        print('worker check:', name, flush=True)
        path = work / (name + '.birnam')
        path.write_text(source)
        environment = env.copy()
        if stdlib:
            environment.pop('BIRNAM_NO_STDLIB')
        result = subprocess.run([binary, path], cwd=work, env=environment,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=600 if stdlib else 180)
        assert result.returncode == 0 and result.stdout == expected, (name, result)
        assert not result.stderr, (name, result.stderr)
    quoted = json.dumps(str(image))
    run('save', f'''
let Handler := [Object clone];
[Reflect defineMethod: #echo: on: Handler as: [:self :v | [GC collectPrecisely]; v]];
[Reflect defineMethod: #fail: on: Handler as: [:self :v | [Error kind: #intentional message: "worker error"]]];
[Reflect defineMethod: #sleep: on: Handler as: [:self :v | [Isolate idleFor: v]; v]];
[Reflect defineMethod: #crash: on: Handler as: [:self :v | [Isolate _isolateCrash: v]]];
[Reflect defineMethod: #untransferable: on: Handler as: [:self :v | [| 42]]];
[System saveImage: {quoted}]
''')
    assert image.is_file()
    start = lambda selector: f'[Isolate start: #Handler selector: #{selector} image: {quoted}]'
    run('sync', f'''
let w := {start('echo:')};
[System println: [w state]];
[System println: [w request: 42]];
let reply := [w request: #[#hello "binary" nil false 1.25]];
[System println: [reply size]];
[System println: [reply at: 1]];
[System println: [[w request: [| 1]] class]];
[w stop]; [System println: [w state]];
''', b'#ready\n42\n5\nbinary\n#encode\n#exited\n')
    run('application-error', f'''
let w := {start('fail:')};let e := [w request: 1];
[System println: [e class]];[System println: [e kind]];
[GC collectPrecisely];[System println: [e message]];
[System println: [e requestId]];[w stop];
''', b'#application\n#intentional\nworker error\n1\n')
    run('result-transfer', f'let w := {start("untransferable:")};[System println: [[w request: nil] kind]];[w stop]', b'#encodeFailed\n')
    run('async', f'''
let w := {start('echo:')};let id := [w submit: nil timeout: 10000];
[System println: [[w submit: 1 timeout: 10000] class]];
let answer := [db cellFor: [Object clone] attr: #reply];[answer set: nil];
[[| [[answer value] isNil]] whileTrue: [| [answer set: [w pollRequest: id]]; [Isolate idleFor: 1]]];
[System println: [[answer value] size]];[System println: [[answer value] at: 0]];
[System println: [[w pollRequest: id] kind]];[w stop];
''', b'#busy\n1\nnil\n#staleRequest\n')
    run('zero-budget', f'''
let w := {start('echo:')};
[System println: [[w request: 99 timeout: 0] class]];
[System println: [w state]];
[System println: [w request: 42]];
[System println: [[w submit: 99 timeout: 0] class]];
[System println: [w state]];
let id := [w submit: 7 timeout: 10000];
[System println: id];
let answer := [db cellFor: [Object clone] attr: #reply]; [answer set: nil];
[[| [[answer value] isNil]] whileTrue: [| [answer set: [w pollRequest: id]]; [Isolate idleFor: 1]]];
[System println: [[answer value] at: 0]]; [w stop];
''', b'#timeout\n#ready\n42\n#timeout\n#ready\n2\n7\n')
    # Expected worker failures go over the socket, without protocol bytes on stdout.
    path = work / 'timeout.birnam'
    path.write_text(f'let w := {start("sleep:")};let e := [w request: 200 timeout: 10];[System println: [e class]];[System println: [w state]]')
    result = subprocess.run([binary, path], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    assert result.returncode == 0 and result.stdout == b'#timeout\n#exited\n', result
    path.write_text(f'let w := {start("crash:")};let e := [w request: nil];[System println: [e exitStatus]];[System println: [w state]]')
    result = subprocess.run([binary, path], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=30)
    assert result.returncode == 0 and result.stdout == b'134\n#exited\n', result

    def frame(channel):
        def exact(n):
            b = b''
            while len(b) < n:
                part = channel.recv(n-len(b))
                assert part, ('unexpected EOF', b)
                b += part
            return b
        header = exact(20)
        magic, version, kind, flags, request, length = struct.unpack('!4sHBBQI', header)
        assert (magic, version, flags) == (b'BISO', 1, 0)
        return kind, request, exact(length)
    # Exercise the exact Ready/Stopped framing and a structured admission failure.
    for receiver, expected in [('Handler', 1), ('MissingHandler', 2)]:
        host, child = socket.socketpair()
        worker = subprocess.Popen([binary, '--worker', image, receiver, 'echo:', '--channel-fd', str(child.fileno())],
                                  env=env, pass_fds=(child.fileno(),), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        child.close();host.settimeout(20)
        kind, request, payload = frame(host)
        assert kind == expected and request == 0, (kind, request, payload)
        if expected == 1:
            assert payload == b''
            host.sendall(struct.pack('!4sHBBQI', b'BISO', 1, 6, 0, 1, 0))
            assert frame(host) == (7, 1, b'')
        else:
            assert b'birnam.boot-error.v1' in payload
        host.close()
        stdout, stderr = worker.communicate(timeout=20)
        assert not stdout
        assert worker.returncode == (0 if expected == 1 else 1), stderr
    # A collected handle becomes an orphan, then checkpoint reaps its process.
    wrapper=work/'worker-launcher'
    pidfile=work/'worker.pid'
    wrapper.write_text('#!/bin/sh\necho $$ > '+shlex.quote(str(pidfile))+'\nexec '+shlex.quote(str(binary))+' "$@"\n')
    wrapper.chmod(0o755)
    marker_done=work/'orphan-drained'
    orphan=work/'orphan.birnam'
    orphan.write_text(f'let discard := [| let h := {start("echo:")}; nil]; [discard value]; [GC collectPrecisely]; [Isolate checkpoint]; [System writeFile: {json.dumps(str(marker_done))} contents: "done"]; [Isolate idleFor: 1000]')
    orphan_env=env.copy();orphan_env['BIRNAM_WORKER_EXECUTABLE']=str(wrapper)
    parent=subprocess.Popen([binary,orphan],env=orphan_env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    try:
        deadline=time.monotonic()+20
        while not marker_done.exists() and time.monotonic()<deadline:
            if parent.poll() is not None:break
            time.sleep(0.01)
        assert marker_done.exists(),parent.communicate(timeout=5)
        pid=int(pidfile.read_text())
        try:os.kill(pid,0)
        except ProcessLookupError:pass
        else:raise AssertionError(('orphan worker survived checkpoint',pid))
        stdout,stderr=parent.communicate(timeout=10)
        assert parent.returncode==0 and not stdout and not stderr,(stdout,stderr)
    finally:
        if parent.poll() is None:parent.kill();parent.wait()

    # Exercise the existing authored TaskService and Worker policy, not a mock server.
    service_image = work / 'service.image'
    service_path = json.dumps(str(service_image))
    marker = work / 'service-entered'
    marker_path = json.dumps(str(marker))
    fixture=(Path(__file__).resolve().parent/'worker-service-smoke.birnam').read_text()
    fixture=fixture[:fixture.index('[Reflect defineMethod: #waitForEntry:')]
    run('save-service', fixture + f"""
[Reflect defineMethod: #library: on: ServiceProbe as: [:self :ignored | [Module libDir]]];
[Module libDir: "/obsolete-install/lib"];
let main := [| [System println: "worker must not execute saved main"]];
[System saveImage: {service_path}];
[System println: [[System imageSaveReport] status]];
""", b'#saved\n', stdlib=True)
    assert service_image.is_file()
    run('worker-library', f"""
let w := [Isolate start: #ServiceProbe selector: #library: image: {service_path}];
[System println: [w request: nil]];[w stop];
""", (str(binary.parent.parent / 'lib') + '\n').encode())
    for mode in ('park','loop'):
        marker=work/('service-entered-'+mode)
        marker_path=json.dumps(str(marker))
        run('service-cancel-'+mode, f"""
let w := [Isolate startService: #ServiceProbe image: {service_path}];
[System println: [w state]];
let id := [w submit: #[#{mode} {marker_path}] timeout: 30000];
[[| [[System readFile: {marker_path}] isError]] whileTrue: [| [w pollRequest: id]; [Isolate idleFor: 1]]];
[System println: [w cancelRequest: id]];
let answer := [db cellFor: [Object clone] attr: #reply];[answer set: nil];
[[| [[answer value] isNil]] whileTrue: [| [answer set: [w pollRequest: id]]; [Isolate idleFor: 1]]];
[System println: [[[answer value] at: 0] at: 0]];
[System println: [System readFile: [{marker_path} , \".done\"]]];
[w stop];[System println: [w state]];
""", b'#ready\ntrue\n#cancelled\ncleaned\n#exited\n')
    run('authored-worker', f"""
let w := [Worker fromHandle: [Isolate startService: #ServiceProbe image: {service_path}]];
let request := [w request: #[#echo 42]];
[System println: [request pollOutcome]];
let failure := [w request: #[#fail]];
[System println: [[[failure pollOutcome] at: 1] size]];
let error := [w request: #[#errorValue]];
[System println: [[[[error pollOutcome] at: 1] at: 0] class]];
[System println: [[w request: #[#echo nil]] pollOutcome]];
[w shutdown];[System println: [w state]];
""", b'#[#success 42]\n1\n#resultTransfer\n#[#success nil]\n#exited\n', stdlib=True)
    print('installed Isolate workers: copied values, errors, async, timeout/crash, orphan reaping and authored service cancellation passed')
