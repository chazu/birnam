#!/usr/bin/env python3
"""Exercise Codegen cache, binding and root callback contracts in fresh processes."""
import argparse
import json
from pathlib import Path
from validation_support import command, save_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    build = command(['cargo', 'test', '--locked', '--offline', '--no-run',
                     '--message-format=json', '-p', 'birnam-codegen-runtime'], timeout=300)
    if build['exit_code']:
        raise RuntimeError(build['output'])
    executables = [row['executable'] for line in build['stdout'].splitlines()
                   if (row := json.loads(line)).get('reason') == 'compiler-artifact'
                   and row.get('executable') and row.get('profile', {}).get('test')
                   and row['target']['name'] == 'birnam_codegen_runtime']
    if len(executables) != 1:
        raise RuntimeError('expected one Codegen runtime test executable')
    rows = []
    for name in ('cache_eviction_preserves_staging_and_failure_side_effects',
                 'binding_publication_retains_both_generations',
                 'root_order_and_source_lock_scope'):
        result = command([executables[0], '--ignored', '--exact', 'state_contract::'+name], timeout=30)
        if result['exit_code'] or '1 passed' not in result['stdout']:
            raise RuntimeError(result['output'])
        rows.append(dict(name=name, result=result))
        print(result['stdout'], end='')
    save_json(args.output, dict(status='passed', rows=rows))


if __name__ == '__main__':
    main()
