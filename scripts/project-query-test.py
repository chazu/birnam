#!/usr/bin/env python3
"""Check real permanent project queries under precise collection in fresh processes."""
import argparse
import json
import os
from pathlib import Path
from validation_support import command, save_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    build = command(['cargo', 'test', '--locked', '--offline', '--no-run', '--lib',
                     '--message-format=json', '-p', 'birnam-host-project'], timeout=300)
    if build['exit_code']:
        raise RuntimeError(build['output'])
    executables = [row['executable'] for line in build['stdout'].splitlines()
                   if (row := json.loads(line)).get('reason') == 'compiler-artifact'
                   and row.get('executable') and row.get('profile', {}).get('test')
                   and row['target']['name'] == 'birnam_host_project']
    if len(executables) != 1:
        raise RuntimeError('expected one native HostProject test executable')
    result = command([executables[0], '--ignored', '--exact',
                      'project_query_contract::project_queries_are_rooted_and_do_not_install_a_global_bridge', '--nocapture'],
                     env=dict(os.environ, BIRNAM_TEST_MODULE_DIRECTORY=str(args.prefix.resolve() / 'lib')),
                     timeout=120)
    if result['exit_code'] or '1 passed' not in result['stdout']:
        raise RuntimeError(result['output'])
    print(result['stdout'], end='')
    save_json(args.output, dict(status='passed', result=result))


if __name__ == '__main__':
    main()
