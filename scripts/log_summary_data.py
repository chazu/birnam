#!/usr/bin/env python3
"""Deterministic synthetic HTTP logs and an independent standard-library oracle."""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import random

SEED = 2977570854

def report(text):
    groups = defaultdict(lambda: [0, 0, 0])
    for number, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        row = json.loads(line)
        if not isinstance(row, dict):
            raise ValueError(f'line {number}: expected a JSON object')
        if not isinstance(row.get('path'), str) or not row['path']:
            raise ValueError(f'line {number}: path must be a nonempty string')
        for key in ('status', 'bytes', 'duration_ms'):
            if type(row.get(key)) is not int:
                raise ValueError(f'line {number}: invalid {key}')
        if not 100 <= row['status'] <= 599 or row['bytes'] < 0 or row['duration_ms'] < 0:
            raise ValueError(f'line {number}: numeric value outside schema')
        bucket = groups[row['path']]
        bucket[0] += 1
        bucket[1] += row['status'] >= 400
        bucket[2] += row['bytes']
    endpoints = [dict(path=path, requests=count, errors=errors, bytes=size)
                 for path, (count, errors, size) in sorted(groups.items())]
    return dict(format='log-summary-v1', requests=sum(r['requests'] for r in endpoints),
                errors=sum(r['errors'] for r in endpoints), bytes=sum(r['bytes'] for r in endpoints),
                endpoints=endpoints)

def encode(rows):
    return ''.join(json.dumps(row, ensure_ascii=False, separators=(',', ':')) + '\n' for row in rows)

def generate(destination):
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    rng = random.Random(SEED)
    paths = ['/'] * 6 + ['/search'] * 4 + ['/api/items', '/health', '/missing', '/café']
    rows = [dict(path=rng.choice(paths), status=rng.choice([200,200,200,201,204,302,404,500,503]),
                 duration_ms=rng.randrange(2000), bytes=rng.randrange(65536)) for _ in range(5000)]
    tiny = [dict(path='/search', status=200, duration_ms=37, bytes=1840),
            dict(path='/', status=503, duration_ms=0, bytes=0),
            dict(path='/search', status=404, duration_ms=7, bytes=120)]
    valid = {'empty':'', 'tiny':encode(tiny), 'small':encode(rows[:50]),
             'medium':encode(rows[:500]), 'large':encode(rows),
             'edges':'\n \t\r\n' + encode(tiny).replace('\n','\r\n').rstrip('\r\n'),
             'escaping':encode([dict(path='/quote"/slash\\/café',status=200,duration_ms=1,bytes=1)])}
    invalid = {'malformed':'{not json}\n', 'missing':encode([dict(path='/a',status=200)]),
               'type':encode([dict(path='/a',status=True,duration_ms=1,bytes=1)]),
               'range':encode([dict(path='/a',status=700,duration_ms=1,bytes=1)]),
               'line-number':encode(tiny[:1])+'\n{bad}\n'}
    manifest = {'seed':SEED, 'generator':'Python random.Random; pinned generated-file hashes',
                'valid':{}, 'invalid':{}}
    for kind, fixtures in [('valid',valid),('invalid',invalid)]:
        for name,text in fixtures.items():
            filename=name+'.jsonl'
            (destination/filename).write_bytes(text.encode())
            record={'file':filename,'sha256':hashlib.sha256(text.encode()).hexdigest()}
            if kind=='valid':
                expected=report(text)
                expected_name=name+'.expected.json'
                (destination/expected_name).write_text(json.dumps(expected,sort_keys=True,indent=2)+'\n')
                record.update(expected=expected_name,records=expected['requests'])
            manifest[kind][name]=record
    (destination/'manifest.json').write_text(json.dumps(manifest,sort_keys=True,indent=2)+'\n')
    return manifest

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory',type=Path)
    args=parser.parse_args()
    generate(args.directory)
