#!/usr/bin/env python3
"""Bounded practical regression screen; no quiet-host or qualified-speedup claim."""
from measurement_archive import load_json
import argparse
import json
from pathlib import Path
import re
import runpy
import statistics
from benchmark_support import Commands, clean_environment, save_json, source_checker
from validation_support import ROOT, sha

APPLICATION = runpy.run_path(str(ROOT / 'scripts/recovery-applications.py'))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--prefix', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    p.add_argument('--switches', nargs='+', choices=tuple(APPLICATION['SWITCH_NAMES']),
                   default=['discarded-clones', 'virtual-fields'])
    p.add_argument('--scenarios', nargs='+', choices=APPLICATION['SCENARIOS'], default=list(APPLICATION['SCENARIOS']))
    p.add_argument('--deadline', type=int, default=600)
    args = p.parse_args()
    if args.deadline <= 0: p.error('deadline must be positive')
    prefix = args.prefix.resolve()
    manifest_path = prefix / 'share/birnam/rust-install.json'
    installed = load_json(manifest_path)
    if installed['profile'] != 'release' or installed['publication']['state'] != 'published':
        p.error('a published release prefix is required')
    artifacts = {prefix / n: h for n, h in installed['staged_manifest'].items()}
    artifacts[manifest_path] = sha(manifest_path)
    sources = {n.removeprefix('share/birnam/src/'): h for n, h in installed['staged_manifest'].items()
               if n.startswith('share/birnam/src/')}
    if not sources: p.error('missing installed source provenance')
    for name in ('scripts/performance-smoke.py', 'scripts/recovery-applications.py',
                 'scripts/benchmark_support.py', 'scripts/validation_support.py', 'bench/native-tier-focus.birnam'):
        sources[name] = sha(ROOT / name)
    current = source_checker(ROOT, sources, artifacts)
    current()
    run = Commands(ROOT, clean_environment(), deadline=args.deadline, timeout=120)
    result = dict(status='running', qualified=False, scope=__doc__, sources=sources,
                  installed_sha256={str(p): h for p,h in artifacts.items()}, switches=args.switches, rows=[], comparisons=[])
    try:
        for scenario in dict.fromkeys(args.scenarios):
            for pair in range(3):
                pair_rows = []
                for enabled in ((0, 1) if pair % 2 == 0 else (1, 0)):
                    current()
                    print(f'performance-smoke: {scenario} pair={pair} enabled={enabled}', flush=True)
                    extra = {APPLICATION['SWITCH_NAMES'][name]: str(enabled) for name in args.switches}
                    output = run([prefix/'bin/birnam', ROOT/'bench/native-tier-focus.birnam'],
                                 dict(extra, NATIVE_TIER_FOCUS=scenario))
                    if output['exit_code']: raise RuntimeError(output['output'])
                    values = APPLICATION['parse_output'](output['stdout'], scenario)
                    row = dict(scenario=scenario, pair=pair, enabled=enabled,
                               execution_us=statistics.median(values['sample-us']), process_wall_ns=output['wall_ns'],
                               stdout=output['stdout'])
                    pair_rows.append(row); result['rows'].append(row); save_json(args.output, result)
                normalized = [re.sub(r'^(?:sample|materialize|warmup)-us \d+$', '', r['stdout'], flags=re.MULTILINE) for r in pair_rows]
                if normalized[0] != normalized[1]: raise RuntimeError('observable output differs across optimization settings')
            for metric in ('execution_us', 'process_wall_ns'):
                changes=[]
                for pair in range(3):
                    off, on = [next(r[metric] for r in result['rows'] if r['scenario']==scenario and r['pair']==pair and r['enabled']==e) for e in (0,1)]
                    if off <= 0: raise RuntimeError('unusable zero-duration sample')
                    changes.append(100 * (on/off - 1))
                median=statistics.median(changes)
                result['comparisons'].append(dict(scenario=scenario, metric=metric, paired_change_percent=changes,
                    median_change_percent=median, slowdown=median > 5 and sum(v > 5 for v in changes) >= 2))
        current()
        result['status'] = 'regression' if any(r['slowdown'] for r in result['comparisons']) else 'passed'
        print(json.dumps(result['comparisons'], indent=2))
        return int(result['status'] != 'passed')
    except BaseException as error:
        result.update(status='failed', error=str(error)); raise
    finally: save_json(args.output, result)


if __name__ == '__main__': raise SystemExit(main())
