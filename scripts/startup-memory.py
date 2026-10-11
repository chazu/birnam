#!/usr/bin/env python3
"""Measure real, fresh-process startup with a trivial unattended script.

Uses warm filesystem caches; this is not a cold-bootstrap build measurement.
An optional RSS ceiling supports a platform-specific regression check.
"""
import argparse
import json
import platform
import re
import statistics
import subprocess
import tempfile
import time
from pathlib import Path
from validation_support import clean_environment, sha


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--max-rss-mib', type=float)
    args = parser.parse_args()
    if not 1 <= args.runs <= 20:
        parser.error('--runs must be 1..20')
    prefix = args.prefix.resolve()
    binary = prefix / 'bin/birnam'
    results = {'prefix': str(prefix), 'platform': platform.platform(),
               'host_sha256': sha(binary),
               'runtime_sha256': {p.name: sha(p) for p in sorted((prefix/'lib').glob('*.dylib'))},
               'image_sha256': {p.name: sha(p) for p in sorted((prefix/'lib').glob('*.image'))},
               'runs': []}
    with tempfile.TemporaryDirectory(prefix='birnam-startup-memory-') as directory:
        work = Path(directory)
        script = work/'ready.birnam'
        script.write_text('[System println: "ready"]\n')
        for mode in ('native-prefix', 'authored-image'):
            for i in range(args.runs):
                env = clean_environment()
                if mode == 'native-prefix':
                    env['BIRNAM_NO_STDLIB'] = '1'
                metrics = work/'time.txt'
                started = time.monotonic_ns()
                run = subprocess.run(['/usr/bin/time', '-l' if platform.system() == 'Darwin' else '-v',
                                      '-o', str(metrics), str(binary), str(script)],
                                     cwd=work, env=env, capture_output=True, timeout=60)
                elapsed = time.monotonic_ns()-started
                if run.returncode or run.stdout != b'ready\n' or run.stderr:
                    raise RuntimeError((run.returncode, run.stdout, run.stderr))
                raw = metrics.read_text()
                if platform.system() == 'Darwin':
                    peak = int(re.search(r'(\d+)\s+maximum resident set size', raw)[1])
                else:
                    peak = int(re.search(r'Maximum resident set size \(kbytes\):\s*(\d+)', raw)[1])*1024
                row = {'mode': mode, 'sample': i, 'wall_ns': elapsed, 'peak_rss_bytes': peak}
                results['runs'].append(row)
                print(mode, i, round(elapsed/1e9, 3), round(peak/(1<<20), 1), flush=True)
    results['medians'] = {
        mode: {key: statistics.median(row[key] for row in results['runs'] if row['mode'] == mode)
               for key in ('wall_ns', 'peak_rss_bytes')}
        for mode in ('native-prefix', 'authored-image')}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(results, indent=2)+'\n')
    if args.max_rss_mib is not None and any(row['peak_rss_bytes'] > args.max_rss_mib*(1<<20)
                                          for row in results['runs']):
        raise SystemExit('startup RSS ceiling exceeded (measurements saved)')


if __name__ == '__main__':
    main()
