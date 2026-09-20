#!/usr/bin/env python3
"""Exercise actual adapter caches against selected, competing and failing images."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
from validation_support import ROOT, command, save_json


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    args = p.parse_args()
    rows = []
    with tempfile.TemporaryDirectory(prefix='birnam-floor-discovery-') as temp:
        temp = Path(temp)
        for name, identity in [('selected', 7), ('competitor', 19)]:
            subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', '-dynamiclib',
                '-I'+str(ROOT/'abi/fixtures/idapi-v13'), '-DFIXTURE_ID='+str(identity),
                str(ROOT/'scripts/fixtures/floor-lookup.c'), '-o', str(temp/(name+'.dylib'))], check=True)
        build = command(['cargo', 'test', '--locked', '--offline', '--no-run', '--message-format=json',
                         '-p', 'birnam-tcc', '-p', 'birnam-codegen-runtime'], timeout=300)
        if build['exit_code']: raise RuntimeError(build['output'])
        executables = {}
        for line in build['stdout'].splitlines():
            row = json.loads(line)
            if row.get('reason') == 'compiler-artifact' and row.get('executable') and row.get('profile', {}).get('test'):
                executables[row['target']['name']] = row['executable']
        for crate, tests in [('birnam_tcc', ['floor_lookup_contract_registration', 'floor_lookup_contract_retirement']),
                             ('birnam_codegen_runtime', ['floor_lookup_contract'])]:
            for test in tests:
                for mode in (0, 1, 2):
                    marker = temp/(crate+'-'+test.replace(':', '-')+'-'+str(mode))
                    env = dict(os.environ, BIRNAM_LOOKUP_BASE=str(args.prefix.resolve()/'lib/libbirnam_id.dylib'),
                               BIRNAM_LOOKUP_SELECTED=str(temp/'selected.dylib'),
                               BIRNAM_LOOKUP_COMPETITOR=str(temp/'competitor.dylib'),
                               BIRNAM_LOOKUP_MODE=str(mode), BIRNAM_LOOKUP_UNLOADED=str(marker))
                    result = command([executables[crate], '--ignored', '--exact', test], env=env, timeout=30)
                    if result['exit_code'] or '1 passed' not in result['stdout']:
                        raise RuntimeError(result['output'])
                    rows.append(dict(crate=crate, test=test, mode=mode, output=result['stdout']))
                    print(f'floor-discovery: {crate} {test} mode={mode} passed', flush=True)
    save_json(args.output, dict(status='passed', rows=rows))


if __name__ == '__main__': main()
