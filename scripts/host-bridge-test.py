#!/usr/bin/env python3
"""Check real host bridge callbacks and address-fenced teardown in fresh processes."""
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
                     '--message-format=json', '-p', 'birnam-host'], timeout=300)
    if build['exit_code']:
        raise RuntimeError(build['output'])
    executables = [row['executable'] for line in build['stdout'].splitlines()
                   if (row := json.loads(line)).get('reason') == 'compiler-artifact'
                   and row.get('executable') and row.get('profile', {}).get('test')
                   and row['target']['name'] == 'birnam_host']
    if len(executables) != 1:
        raise RuntimeError('expected one host test executable')
    rows = []
    for mismatch in (0, 1):
        result = command([executables[0], '--ignored', '--exact',
                          'host_bridge_contract::native_bridge_and_address_fence', '--nocapture'],
                         env=dict(os.environ, BIRNAM_HOST_TEST_PREFIX=str(args.prefix.resolve()),
                                  BIRNAM_HOST_TEST_MISMATCH=str(mismatch)), timeout=60)
        if result['exit_code'] or '1 passed' not in result['stdout']:
            raise RuntimeError(result['output'])
        rows.append(dict(mismatch=bool(mismatch), result=result))
        print(result['stdout'], end='')
    save_json(args.output, dict(status='passed', rows=rows))


if __name__ == '__main__':
    main()
