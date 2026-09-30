"""Verify authored lifecycle intents through a published project's console."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

binary = Path(sys.argv[1]).resolve() / 'bin' / 'birnam'
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
        '[[[Reflect defineMethod: #generationCliValue on: Integer as: [:self | 99]] kind] = #generationNotQuiescent]',
        '[[[Reflect removeMethod: #generationCliValue on: Integer] kind] = #generationNotQuiescent]',
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
    print('generation: running mutation guard, console intent activation and exact rollback passed')
