#!/usr/bin/env python3
"""Full closure coordinate/semantic oracle; compact receipts cannot pass it."""
from pathlib import Path
import argparse
import runpy
from measurement_archive import load_json

ROOT = Path(__file__).resolve().parent.parent

def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--directory', required=True, type=Path)
    p.add_argument('--profile', choices=('debug', 'release'))
    p.add_argument('--all', action='store_true')
    args = p.parse_args()
    checks = runpy.run_path(str(ROOT/'scripts/recovery-graphs.py'))
    if args.all:
        checks['verify_complete_matrices'](args.directory)
        print('complete closure/numeric matrices: exact coverage and semantic assertions passed')
        return
    if not args.profile: p.error('--profile or --all is required')
    check = checks['check_joint_numeric_closure_pair']
    for family in ('sequence', 'budget', 'control', 'inputs'):
        receipts = [load_json(args.directory/f'joint-{family}-{counting}-{args.profile}.json') for counting in ('off','on')]
        check(family, *receipts)
    print('joint numeric closures:', args.profile, 'complete counting off/on comparisons passed')

if __name__ == '__main__': main()
