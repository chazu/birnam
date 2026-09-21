"""Run independent behavioral and rejection oracles through production startup."""
from pathlib import Path
import resource
import subprocess
import sys

resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
host, lib, scratch, group = sys.argv[1:]
owner = 'Rust floor entered from a non-owner thread'
observation = 'Id access during storage observation'
# kind -> (fixture, successful modes with exact output when specified, rejections)
cases = {
    'root-registry': ('root-registry', {'normal': None}, [('overflow', 'gc: too many root providers'), ('observation', observation), ('owner', owner)]),
    'gc-settings': ('gc-settings', {'normal': None}, [(f'{kind}-{setting}', message) for kind, message in [('owner', owner), ('observation', observation)] for setting in ['auto', 'precise']]),
    'native-stack': ('native-stack', {'normal': None}, [(f'{kind}-{value}', message) for kind, message in [('owner', owner), ('observation', observation)] for value in ['zero', 'nonzero']]),
    'native-stack-scan': ('native-stack-scan', {'normal': None}, [('reject', 'GC tracer rejected a candidate')]),
    'vtable-enumeration': ('vtable-enumeration', {'normal': 'vtable enumeration: live append, bounds, weak membership, GC and survivor order passed\n'}, []),
    'vtable-enumeration-boundary': ('vtable-enumeration-boundary', {
        **{f'{op}-collection': 'enumeration remains callable during root collection\n' for op in ['count', 'at']},
        **{f'{op}-observation': 'native enumeration remains callable during storage observation\n' for op in ['count', 'at']},
    }, []),
    'selector-enumeration': ('selector-enumeration', {
        'ordered': 'selector enumeration: ordered growth, overwrite, removal, append, bounds, GC and raw observation passed\n',
        'collection': 'selector observation during root collection passed\n',
        'observation': 'native selector enumeration remains callable during storage observation\n',
        'typed': 'typed selector requests reject non-behaviors before bounds and reject non-Word indices\n',
    }, []),
}
for kind in ['presence', 'import', 'publication']:
    null = 'IdApi.exportObj name is null' if kind == 'publication' else 'IdApi.import name is null'
    cases['namespace-' + kind] = ('namespace-boundary', {'collection': f'{kind} remains callable during root collection\n'}, [
        ('null', null), ('owner', owner), ('null-owner', null), ('observation', observation), ('null-observation', null),
    ])
assert group in ['all', 'root-registry', 'gc-settings', 'native-stack', 'namespace', 'vtable-enumeration', 'selector-enumeration', 'reflection']
passed = 0
for kind, (fixture, successes, rejections) in cases.items():
    selected = group == 'all' or kind == group or kind.startswith(group + '-')
    if group == 'reflection':
        selected = kind.startswith(('namespace-', 'vtable-enumeration', 'selector-enumeration'))
    if not selected:
        continue
    def command(mode):
        return [host, lib, str(Path(scratch) / f'{fixture}.dylib'), mode, kind]
    for mode, output in successes.items():
        result = subprocess.run(command(mode), capture_output=True, text=True)
        assert result.returncode == 0 and not result.stderr, (kind, mode, result)
        if output is not None:
            assert result.stdout == output, (kind, mode, result)
        print(result.stdout, end='', flush=True)
        passed += 1
    for mode, message in rejections:
        result = subprocess.run(command(mode), capture_output=True, text=True)
        assert result.returncode == -6, (kind, mode, result)
        assert result.stderr == 'birnam-id invariant failed: ' + message + '\n', (kind, mode, result)
        expected_stdout = 'candidate 0\n' if kind == 'native-stack-scan' else '32 entries including duplicates\n' if (kind, mode) == ('root-registry', 'overflow') else ''
        assert result.stdout == expected_stdout, (kind, mode, result)
        print(f'{kind} {mode}: exact rejection preserved', flush=True)
        passed += 1
print(f'Runtime services: {passed} production native cases passed', flush=True)
