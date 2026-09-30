"""Retained portable execution semantics through the installed source Host."""
import os
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parent.parent
binary = Path(sys.argv[1]).resolve() / 'bin/birnam'
env = {k: v for k, v in os.environ.items() if not k.startswith('BIRNAM_')}
env['BIRNAM_GC_PRECISE'] = '1'
cases = {
    'portable-aliases': 'copies, argument identity, replacement, eviction and GC',
    'portable-projection': 'portable projections, captures, identity, eviction and GC',
    'portable-effects': 'aliases, captures, effects, eviction and GC',
    'portable-numbers': 'arithmetic, mixed values, errors, identity and retirement',
}
for name, coverage in cases.items():
    result = subprocess.run([binary, root / 'scripts/fixtures' / (name + '.birnam')],
                            cwd=root, env=env, stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, timeout=300)
    assert result.returncode == 0 and not result.stderr, (name, result)
    assert result.stdout == f'{name}: {coverage} passed\n', (name, result)
    print(result.stdout, end='', flush=True)
print('portable execution: four retained semantic fixtures passed')
