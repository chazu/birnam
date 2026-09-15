#!/usr/bin/env python3
"""Characterize actual relational output adapters against a real installed floor."""
import argparse
import json
import os
import re
from pathlib import Path
from validation_support import command, save_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    build = command(['cargo', 'test', '--locked', '--offline', '--no-run',
                     '--message-format=json', '-p', 'birnam-relational'], timeout=300)
    if build['exit_code']:
        raise RuntimeError(build['output'])
    executables = [row['executable'] for line in build['stdout'].splitlines()
                   if (row := json.loads(line)).get('reason') == 'compiler-artifact'
                   and row.get('executable') and row.get('profile', {}).get('test')
                   and row['target']['name'] == 'birnam_relational']
    if len(executables) != 1:
        raise RuntimeError('expected one relational test executable')
    result = command([executables[0], '--ignored', '--exact',
                      'native_output::exported_output_contract', '--nocapture'],
                     env=dict(os.environ, BIRNAM_RELATIONAL_FLOOR=str(
                         args.prefix.resolve()/'lib/libbirnam_id.dylib')), timeout=30)
    if result['exit_code'] or '1 passed' not in result['stdout']:
        raise RuntimeError(result['output'])
    print(result['stdout'], end='')
    rows = [dict(zip(('family', 'case', 'rust_allocations', 'floor_live_delta'), map(int, match)))
            for match in re.findall(r'native-output-allocation family=(\d+) case=(\d+) rust=(\d+) floor=(\d+)', result['stdout'])]
    if len(rows) != 120 or len({(r['family'], r['case']) for r in rows}) != 120:
        raise RuntimeError('incomplete native adapter matrix')
    save_json(args.output, dict(status='passed', adapters=10, cases=120, rows=rows, result=result))


if __name__ == '__main__':
    main()
