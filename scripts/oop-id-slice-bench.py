#!/usr/bin/env python3
"""Serial matched C workload for the isolated Oop-only migration slice."""
from pathlib import Path
import argparse, hashlib, json, statistics, subprocess

root = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--baseline', type=Path, default=Path.home()/'.local/opt/birnam')
p.add_argument('--output', type=Path, default=root/'.cargo-target/oop-slice/qualification/final-timings.json')
a = p.parse_args()
target = root/'.cargo-target/oop-slice'
libraries = ['id', 'root_registry', 'collect']
receipts = {}
for lane, lib in [('v1', a.baseline/'lib'), ('v2', target/'release')]:
    receipts[lane] = {str(lib/f'libbirnam_{name}.dylib'): hashlib.sha256((lib/f'libbirnam_{name}.dylib').read_bytes()).hexdigest() for name in libraries}
runs = []
for i, lane in enumerate(['v1','v2','v2','v1','v1','v2','v2','v1']):
    lib = a.baseline/'lib' if lane == 'v1' else target/'release'
    argv = [str(target/'fixtures'/f'bench-{lane}'), *[str(lib/f'libbirnam_{name}.dylib') for name in libraries]]
    result = subprocess.run(argv, check=True, capture_output=True, text=True)
    runs.append(dict(order=i, lane=lane, argv=argv, observations=[json.loads(line) for line in result.stdout.splitlines()]))
summary = {}
for case in ['allocate-and-retain-eighth','intern-existing-64','produce-and-intern-new-64','collect-garbage']:
    medians = {lane: statistics.median(row['ns_per_op'] for run in runs if run['lane']==lane for row in run['observations'] if row.get('case')==case) for lane in ['v1','v2']}
    summary[case] = dict(median_ns=medians, change_percent=100*(medians['v2']/medians['v1']-1))
for lane in receipts:
    for file, digest in receipts[lane].items():
        assert hashlib.sha256(Path(file).read_bytes()).hexdigest()==digest, f'Library changed during timing: {file}'
a.output.parent.mkdir(parents=True, exist_ok=True)
a.output.write_text(json.dumps(dict(library_sha256=receipts, runs=runs, summary=summary, limits=[
    'Thread CPU clock; four fresh processes per variant; no confidence interval.',
    'Same C caller workload, different implementation work: candidate includes invocation roots and Method objects.',
    'Fresh-name candidate constructs a Name object from Oop bytes; baseline consumes a raw byte span.',
    'GC compares different heap populations; report heap/reclamation counts with timings.',
    'Automatic collection is not enabled; this is not Logue or full-language acceptance.'
]), indent=2)+'\n')
print(json.dumps(summary, indent=2))
