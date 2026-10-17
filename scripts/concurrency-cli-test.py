"""Exercise authored Concurrency preparation and service ownership after installation."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve() / 'bin/birnam'
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env['BIRNAM_GC_PRECISE'] = '1'
with tempfile.TemporaryDirectory(prefix='birnam-concurrency-') as temporary:
    work = Path(temporary)
    fixture = work / 'service.birnam'
    fixture.write_bytes((Path(__file__).parent / 'worker-service-smoke.birnam').read_bytes())
    invalid = work / 'invalid.birnam'
    invalid.write_text(']')
    quote = lambda p: json.dumps(str(p))
    source = work / 'run.birnam'
    source.write_text(f'''
[Module load: #os];
[System println: [[Concurrency load: #[{quote(invalid)}]] kind]];
[System println: [[Concurrency load: #[{quote(work / 'missing.birnam')}]] isError]];
let program := [Concurrency load: #[{quote(fixture)}]];
[[program isError] ifTrue: [| [System println: program]; [OS exit: 1]]];
[System println: [[program field: #workspace] value]];
// Worker startup must use the captured image, after its source disappears.
[{quote(fixture)} concurrencyDelete];
[System println: [[DispatchGeneration current] isNil]];
let another := [Concurrency load: #[]];
[System println: [[another isError] not]];
[System println: [[another field: #workspace] value]];
[another close];
let passed := [ServiceCheck run: program];
let closed := [program close];
[System println: [[closed pollOutcome] at: 0]];
[System println: [[program worker: #ServiceProbe] kind]];
[[passed = true] ifFalse: [| [OS exit: 1]]];
''')
    result = subprocess.run([binary, source], cwd=work, env=env,
                            capture_output=True, timeout=600)
    assert result.returncode == 0 and not result.stderr, (result.returncode, result.stdout, result.stderr)
    lines = result.stdout.decode().splitlines()
    assert lines[:2] == ['#invalidConcurrencyModule', 'true'], lines
    assert lines[3:5] == ['true', 'true'], lines
    assert lines[6:] == ['#[#workerServiceChecks 19 #failures 0]',
                         '#success', '#concurrencyClosed'], lines
    assert not fixture.exists(), 'worker fixture source survived deletion'
    workspace = Path(lines[2])
    assert workspace.is_absolute() and workspace.name.startswith('birnam-concurrency-'), lines
    assert not workspace.exists(), ('close left worker image/workspace behind', workspace)
    another_workspace = Path(lines[5])
    assert another_workspace != workspace and another_workspace.name.startswith('birnam-concurrency-'), lines
    assert not another_workspace.exists(), ('second preparation left its workspace behind', another_workspace)
print('installed Concurrency: batch admission, source-free workers, 19 service checks and cleanup passed')
