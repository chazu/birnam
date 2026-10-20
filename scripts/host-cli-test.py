"""Accept an installed prefix; move it and execute from an unrelated directory."""
import json
import os
from pathlib import Path
import subprocess
import sys
from install_contract import verify

prefix = Path(sys.argv[1]).resolve()
receipt = verify(prefix)
assert receipt['admission_contract']['host_commands'] == ['source', 'image', 'worker', 'compile-module', 'artifact', 'check', 'build', 'run', 'fetch', 'install', 'test', 'repl', 'listen', 'lsp', 'doc', 'docgen', 'init', 'wrap', 'bake', 'rebuild', 'help', 'version']
assert receipt['bootstrap_modules'] == ['error', 'globalsOwner', 'sourceRegistry', 'cell', 'modules', 'lang', 'nativeValues', 'booleanValues', 'integerValues', 'blockLoopControl', 'blockLoop', 'range', 'systemGc', 'printing', 'sourceLoader', 'nativeLoader', 'alienCompiler', 'alienRecipes', 'imageFormat', 'trace', 'globals', 'methodInvocation']
assert receipt['build_seed'] == {'grammar': 'birnam-lang-seed', 'published': False}
assert 'libbirnam_lang.dylib' not in receipt['dynamic_libraries']
assert (prefix / 'lib/libbirnam_lang.dylib').read_bytes().startswith(b'\x7fELF')
assert not (prefix / 'lib/libbirnam_lang_seed.dylib').exists()
assert receipt['stage1_modules'] == ['stdlib', 'stacklessControl', 'candidatePolicy', 'compiledUnit', 'toml', 'config', 'doc', 'cli', 'file', 'html', 'http', 'json', 'log', 'lsp', 'markdown', 'os', 'process', 'random', 'repl', 'socket', 'time', 'nativedoc', 'compiledUnitDocs', 'concurrencyDocs']
assert receipt['source_bootstrap_modules'] == ['stdlib', 'stacklessControl', 'candidatePolicy', 'compiledUnit', 'isolate']
assert 'libbirnam_storage.dylib' in receipt['dynamic_libraries']
assert 'libbirnam_trace.dylib' not in receipt['dynamic_libraries']
assert (prefix / 'lib/libbirnam_trace.dylib').read_bytes().startswith(b'\x7fELF')
relocated = prefix.parent / 'relocated'
prefix.rename(relocated)
work = relocated.parent / 'working elsewhere'
work.mkdir()
binary = relocated / 'bin/birnam'
launcher = work / 'launcher'
launcher.symlink_to(binary)
environment = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
environment['BIRNAM_GC_PRECISE'] = '1'

def run(args, *, stdlib=False, executable=binary, extra=None):
    env = environment.copy()
    if not stdlib:
        env['BIRNAM_NO_STDLIB'] = '1'
    env.update(extra or {})
    return subprocess.run([executable, *args], cwd=work, env=env,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=300)

def source(name, text, *, output=b'', diagnostic=None, stdlib=False):
    path = work / name
    path.write_bytes(text)
    result = run([path], stdlib=stdlib)
    assert result.stdout == output, (name, result.stdout, result.stderr)
    if diagnostic is None:
        assert result.returncode == 0 and result.stderr == b'', (name, result)
    else:
        assert result.returncode == 1 and diagnostic in result.stderr, (name, result)
        assert os.fsencode(path) in result.stderr or '\udcff' in str(path), result.stderr

for option in ['--help', '--version']:
    result = run([option], extra={'BIRNAM_ID_DYLIB': '/missing/id.dylib'})
    assert result.returncode == 0 and result.stdout and not result.stderr, result
source('nil.birnam', b'nil')
source('bootstrap-json.birnam', b'[Module load: #json]; [System println: [#hello asJson]]; [System println: [nil asJson]]',
       output=b'"#hello"\nnull\n', stdlib=True)
source('false.birnam', b'false')
source('output.birnam', b'[System print: "hello"]; [System println: " world"]', output=b'hello world\n')
source('syntax.birnam', b'[System println:', diagnostic=b'syntax error')
source('error.birnam', b'[System readFile: "/nonexistent/birnam-host-test-file"]', diagnostic=b'could not read file')
source('alien.birnam', b'[Alien wrap: \"strlen\" signature: \"l s\" as: #nativeLength on: String]; [System println: [\"hello\" nativeLength]]', output=b'5\n')
source('gc.birnam', b'let value := "alive"; [System gc]; [System println: value]', output=b'alive\n')
(work / 'asset').write_bytes(b'embedded content')
source('embedding.birnam', b'[System println: [[System embedFile: "asset"] size]]', output=b'16\n')
source('unicode-λ.birnam', b'[System println: "path works"]', output=b'path works\n')
result = run([work / 'absent.birnam'])
assert result.returncode == 1 and b'absent.birnam' in result.stderr and not result.stdout
large = work / 'large.birnam'
with large.open('wb') as file:
    file.truncate(16 * 1024 * 1024 + 1)
result = run([large])
assert result.returncode == 1 and b'exceeds' in result.stderr
for args in [[], ['--repl']]:
    result = run(args, extra={'BIRNAM_ID_DYLIB': '/missing/id.dylib'})
    assert result.returncode == 1 and b'the REPL requires the stdlib' in result.stderr, result
result = run(['nil.birnam', 'extra'])
assert result.returncode == 1 and b'one file' in result.stderr
result = run(['nil.birnam'], extra={'BIRNAM_ID_DYLIB': '/missing/id.dylib'})
assert result.returncode == 1 and b'native status=' in result.stderr
result = run(['output.birnam'], executable=launcher)
assert result.returncode == 0 and result.stdout == b'hello world\n' and not result.stderr
stdlib_image = work / 'stdlib.image'
source('stdlib.birnam', b'[System println: [Module isLoaded: #stdlib]]; [System gc]; [System println: [#[1 2 3] collect: [:x | [x + 1]]]]; '
       b'let savedLifecycleUnit := [CompiledUnit clone]; [savedLifecycleUnit state: #historical]; '
       b'[Globals at: #main put: [| [System gc]; [System println: [#[1 2 3] collect: [:x | [x + 1]]]]; [System println: [Module libDir]]; [System println: [[savedLifecycleUnit state] = #historical]]]]; '
       b'[Module libDir: "/old-install/lib"]; [System saveImage: ' + json.dumps(str(stdlib_image)).encode() + b']',
       stdlib=True, output=b'true\n#[2 3 4]\n')
assert stdlib_image.is_file()
result = run([stdlib_image], stdlib=True)
assert result.returncode == 0 and not result.stderr, result
assert result.stdout == b'#[2 3 4]\n' + os.fsencode(relocated / 'lib') + b'\ntrue\n', result
print('installed Host/CLI: receipt, relocation, source, diagnostics, GC, stdlib and early mode validation passed')
