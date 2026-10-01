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

# Metadata and code share one parse, and later compilations observe replacement.
parse_artifact = work / 'parse-count.dylib'
rejected_artifact = work / 'parse-rejected.dylib'
parse_probe = work / 'parse-count.birnam'
parse_probe.write_text(f'''
let ArtifactOriginalParse := [Lang methodFor: #parse:];
let ArtifactParseCount := 0;
[Reflect defineMethod: #parse: on: Lang as: [:self :source |
  [Globals at: #ArtifactParseCount put: [[Globals at: #ArtifactParseCount] + 1]];
  [ArtifactOriginalParse invokeOn: self arguments: #[source]]]];
[Codegen compile: "[Module named: #parseCount requires: #[]];" output: {quote(parse_artifact)}];
[System println: [Globals at: #ArtifactParseCount]];
[Codegen compile: "[Module named: #parseCount requires: #[]];" output: {quote(parse_artifact)}];
[System println: [Globals at: #ArtifactParseCount]];
[Reflect defineMethod: #parse: on: Lang as: [:self :source | nil]];
[System println: [[Codegen compile: "[Module named: #rejected requires: #[]];" output: {quote(rejected_artifact)}] isNil]]
''')
result = run([parse_probe])
assert result.returncode == 1 and result.stdout == b'1\n2\ntrue\n', result
assert b'syntax error' in result.stderr, result
assert parse_artifact.is_file() and not rejected_artifact.exists()

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

# Nested source loading publishes globals to a caller compiled before that load.
# Both the source runner and the source-free compiled runner must execute them.
leaf_source = work / 'nested-leaf.birnam'
leaf_source.write_text('let NestedAnswer := 73; true')
outer_source = work / 'nested-outer.birnam'
outer_source.write_text(f'[System load: {quote(leaf_source)}]; '
                        'let NestedReader := [| NestedAnswer]; true')
runner_text = ('let readLater := [| [NestedReader value]]; '
               f'[System load: {quote(outer_source)}]; [System gc]; '
               '[System println: [readLater value]]; '
               '[Globals at: #NestedAnswer put: 91]; [System gc]; '
               '[System println: [readLater value]]; true')
runner_source = work / 'nested-runner.birnam'
runner_source.write_text(runner_text)
succeeds(runner_source, b'73\n91\n')
runner_source.unlink()
runner_artifact = compile('nested-runner', runner_text)
succeeds(runner_artifact, b'73\n91\n')

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
data = wrong.read_bytes().replace(b'id-object-v2', b'old-object-x')
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
    f'[System println: [[[Codegen load: {quote(registered)}] kind] = #moduleIntroductionNotAvailable]]')
result = run([probe])
assert result.returncode == 0 and not result.stderr, result
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
# Ordinary Id artifacts already use resumable entries. The retired generation
# bootstrap switch is rejected by argument parsing before opening source/runtime.
result = run(['--compile-module', 'missing.birnam', 'unused.dylib', '--initial-generation-resumable'])
assert result.returncode == 1 and b'Arity' in result.stderr and b'actual: 3' in result.stderr, result
print('installed artifacts: inert compile, removed source/assets, nested globals, dependency order, preflight and failures passed')
