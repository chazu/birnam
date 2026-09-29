"""Durable compiled modules: fresh-process activation and closure preflight."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

prefix = Path(sys.argv[1]).resolve()
scratch = tempfile.TemporaryDirectory(prefix='artifact checks ', dir=prefix.parent)
work = Path(scratch.name)
binary = prefix / 'bin/birnam'
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env['BIRNAM_GC_PRECISE'] = '1'

def quote(path):
    return json.dumps(str(path))

def run(args):
    return subprocess.run([binary, *args], cwd=work, env=env,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=300)

def compile(name, text, output=None):
    source = work / (name + '.birnam')
    artifact = work / (output or ('libbirnam_' + name + '.dylib'))
    source.write_text(text)
    result = run(['--compile-module', source, artifact])
    assert result.returncode == 0 and not result.stdout and not result.stderr, result
    assert artifact.read_bytes().startswith(b'\x7fELF')
    source.unlink()
    return artifact

def succeeds(path, stdout):
    result = run([path])
    assert result.returncode == 0 and result.stdout == stdout and not result.stderr, result

def fails(path, diagnostic):
    result = run([path])
    assert result.returncode == 1 and not result.stdout and diagnostic in result.stderr, result

# Compilation is inert; relative binary embedding is fixed at compile time.
(work / 'asset').write_bytes(b'owned\0\xff')
marker = work / 'executed'
root = compile('main', '[Module named: #artifactMain requires: #[]]; '
    f'[System writeFile: {quote(marker)} contents: "yes"]; '
    '[Globals at: #saved put: [| [System embedFile: "asset"]]]; '
    '[System gc]; [System println: [[saved value] size]]; true', 'chosen.dylib')
assert not marker.exists()
(work / 'asset').unlink()
succeeds(root, b'7\n')
assert marker.read_text() == 'yes'

# Dependency-first order, diamond deduplication, arbitrary root filename.
leaf = compile('artifactLeaf', '[Module named: #artifactLeaf requires: #[]]; [System println: "leaf"]; true')
left = compile('artifactLeft', '[Module named: #artifactLeft requires: #[#artifactLeaf]]; [System println: "left"]; true')
right = compile('artifactRight', '[Module named: #artifactRight requires: #[#artifactLeaf]]; [System println: "right"]; true')
root = compile('root', '[Module named: #artifactRoot requires: #[#artifactLeft #artifactRight]]; [System println: "root"]; true', 'root.dylib')
succeeds(root, b'leaf\nleft\nright\nroot\n')

# Artifact dependency inventory remains broader than revision admission.
relaxed = compile('relaxed', '[Module named: #relaxed requires: #[#artifactLeaf]]; '
    '[Reflect defineMethod: #answer on: [Object clone] as: [:self | 42]]; '
    '[System println: "relaxed"]; true', 'arbitrary.dylib')
succeeds(relaxed, b'leaf\nrelaxed\n')

# A later invalid dependency must prevent all earlier initialization.
marker.unlink()
early = compile('artifactEarly', '[Module named: #artifactEarly requires: #[]]; '
    f'[System writeFile: {quote(marker)} contents: "bad"]; true')
root = compile('missingRoot', '[Module named: #missingRoot requires: #[#artifactEarly #absentDependency]]; true')
fails(root, b'could not inspect artifact')
assert not marker.exists()
# Identity mismatch also fails during plan construction.
wrong = compile('absentDependency', '[Module named: #differentIdentity requires: #[]]; true')
fails(root, b'identity does not match')
assert not marker.exists()
wrong.unlink()
# Legacy descriptors cannot enter the plan, even when their ABI/target match.
wrong = compile('absentDependency', '[Module named: #absentDependency requires: #[]]; true')
data = wrong.read_bytes().replace(b'id-object-v1', b'old-object-x')
wrong.write_bytes(data)
fails(root, b'no Id native-object contract')
assert not marker.exists()
# Cycles and adjacent native requirements are inspection failures too.
wrong = compile('absentDependency', '[Module named: #absentDependency requires: #[#missingRoot]]; true')
fails(root, b'circular dependency')
assert not marker.exists()
wrong = compile('absentDependency', '[Module named: #absentDependency requires: #[]]; [Alien dlopen: "missing-native.dylib"]; true')
fails(root, b'could not inspect artifact')
assert not marker.exists()
# Ordinary activation records original source and rejects a conflicting revision.
registered_text = '[Module named: #registered requires: #[]]; true'
registered = compile('registered', registered_text)
conflict = compile('conflict', '[Module named: #registered requires: #[]]; '
    f'[System writeFile: {quote(marker)} contents: "bad"]; true')
probe = work / 'registry.birnam'
probe.write_text(f'[Codegen load: {quote(registered)}]; '
    '[System println: [Codegen moduleSourceFor: #registered]]; '
    f'[System println: [[Codegen load: {quote(conflict)}] isNil]]; '
    '[System println: [Codegen lastError]]; '
    '[Codegen sealInitialGeneration]; '
    f'[System println: [[Codegen load: {quote(registered)}] isNil]]')
result = run([probe])
assert result.returncode == 1 and b'introduction is not available' in result.stderr, result
assert result.stdout == registered_text.encode() + b'\ntrue\nRustCodegen duplicate module source\ntrue\n', result
assert not marker.exists()
# Direct Codegen loading requires already loaded module dependencies.
probe.write_text(f'[System println: [[Codegen load: {quote(left)}] isNil]]; '
                 '[System println: [Codegen lastError]]')
result = run([probe])
assert result.returncode == 1 and b'requirement is unavailable' in result.stderr, result
assert result.stdout == b'true\nRustCodegen artifact module requirement is unavailable\n', result
# Runtime errors and nil initialization fail; false is an ordinary success.
error = compile('runtimeFailure', '[System readFile: "/nonexistent/birnam-artifact-test"]')
fails(error, b'could not read file')
empty = compile('nilResult', 'nil')
fails(empty, b'could not install module')
false = compile('false', 'false')
succeeds(false, b'')
# The old generation-selection flag is still explicitly outside this slice.
result = run(['--compile-module', 'missing.birnam', 'unused.dylib', '--initial-generation-resumable'])
assert result.returncode == 1 and b'initial-generation selection awaits migration' in result.stderr, result
print('installed artifacts: inert compile, removed source/assets, dependency order, preflight and failures passed')
