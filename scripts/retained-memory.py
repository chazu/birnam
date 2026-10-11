#!/usr/bin/env python3
"""macOS post-GC VM/allocator census, using an unattended stdin handshake."""
import argparse
import json
import platform
import re
import select
import subprocess
import tempfile
from pathlib import Path
from validation_support import clean_environment, sha


def size(text):
    value, unit = re.fullmatch(r'([\d.]+)([KMGT]?)', text).groups()
    return round(float(value) * 1024 ** ('KMGT'.index(unit)+1 if unit else 0))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--max-footprint-mib', type=float)
    args = parser.parse_args()
    if platform.system() != 'Darwin':
        parser.error('this census requires macOS vmmap')
    if not 1 <= args.runs <= 20:
        parser.error('--runs must be 1..20')
    prefix = args.prefix.resolve()
    result = {'platform': platform.platform(), 'prefix': str(prefix),
              'qualification': 'Fresh processes held after explicit GC. vmmap sizes are rounded; RSS and physical footprint are distinct measures.',
              'host_sha256': sha(prefix/'bin/birnam'),
              'runtime_sha256': {p.name: sha(p) for p in sorted((prefix/'lib').glob('*.dylib'))},
              'image_sha256': {p.name: sha(p) for p in sorted((prefix/'lib').glob('*.image'))},
              'samples': []}
    with tempfile.TemporaryDirectory(prefix='birnam-retained-memory-') as directory:
        work = Path(directory)
        source = work/'held.birnam'
        source.write_text('[System gc]; [System println: "ready"]; [System readLine]\n')
        for _ in range(args.runs):
            process = subprocess.Popen([str(prefix/'bin/birnam'), str(source)], cwd=work,
                                       env=clean_environment(), stdin=subprocess.PIPE,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            try:
                if not select.select([process.stdout], [], [], 30)[0]:
                    raise RuntimeError('startup handshake timed out')
                if process.stdout.readline() != b'ready\n':
                    raise RuntimeError('startup handshake failed')
                rss = int(subprocess.check_output(['/bin/ps', '-o', 'rss=', '-p', str(process.pid)], timeout=10))*1024
                raw = subprocess.check_output(['/usr/bin/vmmap', '-summary', str(process.pid)],
                                              stderr=subprocess.STDOUT, text=True, timeout=30)
                footprint = size(re.search(r'^Physical footprint:\s+(\S+)', raw, re.M)[1])
                zones = raw[raw.rindex('MALLOC ZONE'):]
                total = next(line.split() for line in zones.splitlines() if line.startswith('TOTAL '))
                row = {'rss_bytes': rss, 'physical_footprint_bytes': footprint,
                       'malloc_allocated_bytes': size(total[6]),
                       'malloc_dirty_fragmentation_bytes': size(total[7]), 'vmmap': raw}
                process.stdin.write(b'\n'); process.stdin.flush()
                out, err = process.communicate(timeout=10)
                if process.returncode or out or err:
                    raise RuntimeError((process.returncode, out, err))
                result['samples'].append(row)
                print({k: round(v/(1<<20), 1) for k, v in row.items() if k.endswith('_bytes')}, flush=True)
            finally:
                if process.poll() is None:
                    process.kill(); process.communicate()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    if args.max_footprint_mib is not None and any(
            row['physical_footprint_bytes'] > args.max_footprint_mib*(1<<20)
            for row in result['samples']):
        raise SystemExit('retained footprint ceiling exceeded (measurements saved)')


if __name__ == '__main__':
    main()
