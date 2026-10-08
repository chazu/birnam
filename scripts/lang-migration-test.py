"""Compare authored Lang with its build seed, and audit the production closure."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parent.parent
prefix = Path(sys.argv[1]).resolve()
binary = prefix / 'bin/birnam'
lib = prefix / 'lib'
assert (lib / 'libbirnam_lang.dylib').read_bytes().startswith(b'\x7fELF')
assert not (lib / 'libbirnam_lang_seed.dylib').exists()
receipt = json.loads((prefix / 'share/birnam/rust-install.json').read_text())
assert receipt['bootstrap_modules'] == ['error', 'modules', 'lang', 'nativeValues', 'booleanValues', 'integerValues', 'blockLoopControl', 'blockLoop', 'range', 'systemGc', 'printing', 'sourceLoader', 'nativeLoader', 'alienCompiler', 'imageFormat']
assert 'libbirnam_lang.dylib' not in receipt['dynamic_libraries']
assert receipt['build_seed']['published'] is False

target = root / '.cargo-target/lang-seed-test'
subprocess.run(['cargo', 'build', '--manifest-path', str(root / 'Cargo.toml'),
                '--target-dir', str(target), '--offline', '--locked', '--release',
                '-p', 'birnam-bootstrap', '-p', 'birnam-lang-seed',
                '--features', 'birnam-bootstrap/lang-seed'], check=True)
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env.update(BIRNAM_NO_STDLIB='1', BIRNAM_GC_PRECISE='1')

with tempfile.TemporaryDirectory(prefix='birnam-lang-') as directory:
    work = Path(directory)
    seed_lib = work / 'seed'; seed_lib.mkdir()
    for p in lib.iterdir(): (seed_lib / p.name).symlink_to(p)
    (seed_lib / 'libbirnam_bootstrap.dylib').unlink()
    (seed_lib / 'libbirnam_bootstrap.dylib').symlink_to(target / 'release/libbirnam_bootstrap.dylib')
    (seed_lib / 'libbirnam_lang_seed.dylib').symlink_to(target / 'release/libbirnam_lang_seed.dylib')
    fixtures = [
        '', ' // comment', 'nil', '42', '-17', '3.14', 'let x := 1\n[x + 2]',
        '[1 + 2]; [3 * 4];', '[:x :y | [x + y]]', '#[1 #symbol "escaped\\\"text"]',
        '{#a -> 1, #b -> #[2 3]}', '?variable', '#keyword:selector:', '#=>',
        '[x => foo => + 1 => at: 0]', '[x => // marker\nfoo]', '[1 =>+ 2]',
        '[m at: k put: [n => foo]]', '[| [1 + 1] [2 + 2]]',
        '[', 'let := 3', '[x =>]', '[x =>foo]', '[x => => foo]',
        '[x + 1 => foo]', '[x foo => bar]', '"unterminated', 'let x :=',
        '[1 +]', '#[1 2', '{#a ->}', '/* no block comments */',
    ]
    sources = []
    for i, text in enumerate(fixtures):
        p = work / f'case-{i}.birnam'; p.write_text(text); sources.append(p)
    sources += sorted((root / 'modules').glob('*.birnam'))
    sources += sorted((root / 'examples').glob('*.birnam'))
    corpus = work / 'corpus.txt'
    corpus.write_text(''.join(str(p) + '\n' for p in sources))
    subprocess.run(['cargo', 'test', '--manifest-path', str(root / 'Cargo.toml'),
                    '--target-dir', str(root / '.cargo-target/lang-proof'),
                    '--offline', '--locked', '-p', 'birnam-siphon', '--test', 'lang',
                    '--', '--include-ignored', '--nocapture'],
                   env={**env, 'BIRNAM_TEST_MODULE_DIRECTORY': str(seed_lib),
                        'BIRNAM_LANG_CORPUS': str(corpus)}, check=True, timeout=600)

    # Seed and authored parsers must compile Lang itself to identical artifact
    # bytes using the same source/output paths, including compiled adapters.
    artifact = work / 'self.dylib'
    args = [binary, '--compile-module', root / 'modules/lang.birnam', artifact]
    compiled = subprocess.run(args, cwd=work, env={**env, 'BIRNAM_LIB_DIR': str(seed_lib)}, capture_output=True, timeout=120)
    assert compiled.returncode == 0 and not compiled.stderr, compiled
    seed_bytes = artifact.read_bytes()
    compiled = subprocess.run(args, cwd=work, env=env, capture_output=True, timeout=120)
    assert compiled.returncode == 0 and not compiled.stderr, compiled
    assert artifact.read_bytes() == seed_bytes, 'seed/authored Lang self-compilation differs'
    print('Lang: seed and authored grammar produce identical self-compilation artifacts', flush=True)

    probe = work / 'production.birnam'
    probe.write_text('[System println: [Codegen evalPersisting: "[1 + 2]"]]; true')
    live = subprocess.run([binary, probe], cwd=work,
        env={**env, 'DYLD_PRINT_LIBRARIES': '1'}, capture_output=True, timeout=30)
    assert live.returncode == 0 and live.stdout == b'3\n', live
    assert b'libbirnam_lang_seed' not in live.stderr, live.stderr
    assert b'/libbirnam_lang.dylib' not in live.stderr, live.stderr
    # A present seed does not rescue a missing compiled Lang in normal bootstrap.
    (seed_lib / 'libbirnam_bootstrap.dylib').unlink()
    (seed_lib / 'libbirnam_bootstrap.dylib').symlink_to(lib / 'libbirnam_bootstrap.dylib')
    (seed_lib / 'libbirnam_lang.dylib').unlink()
    missing = subprocess.run([binary, probe], cwd=work,
        env={**env, 'BIRNAM_LIB_DIR': str(seed_lib)}, capture_output=True, timeout=30)
    assert missing.returncode != 0 and not missing.stdout, missing
    print('Lang: production excludes the Rust grammar and rejects missing compiled Lang', flush=True)
