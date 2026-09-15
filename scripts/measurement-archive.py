#!/usr/bin/env python3
"""Archive historical JSON losslessly, verify stubs, or extract original receipts."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile
from measurement_archive import SCHEMA, read_bytes, load_json


def archive(directory, name):
    destination = directory / name
    if destination.exists(): raise ValueError('archive already exists; choose a new name')
    rows = []
    for path in sorted(directory.glob('*.json')):
        data = path.read_bytes()
        document = json.loads(data)
        if document.get('archive_schema') == SCHEMA: continue
        row = dict(archive_schema=SCHEMA, archive=name, member=path.name,
                   bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
        for key in ('status', 'design', 'qualified', 'production_verification', 'scope'):
            if key in document and isinstance(document[key], (str, bool, int)):
                row[key] = document[key]
        rows.append((path, data, row))
    # Verify every archived payload before replacing any source receipt.
    with zipfile.ZipFile(destination, 'x', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as output:
        for path, data, row in rows:
            info = zipfile.ZipInfo(path.name, date_time=(2026, 9, 14, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            output.writestr(info, data)
    with zipfile.ZipFile(destination) as output:
        for path, data, row in rows:
            if output.read(path.name) != data: raise ValueError('archive verification failed')
    for path, data, row in rows:
        path.write_text(json.dumps(row, indent=2) + '\n')
    return dict(files=len(rows), original_bytes=sum(len(data) for _, data, _ in rows), archive_bytes=destination.stat().st_size)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('pack', 'verify', 'extract'))
    parser.add_argument('--directory', type=Path, default=Path('docs/measurements'))
    parser.add_argument('--name', default='archive-20260914.zip')
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.action == 'pack':
        print(json.dumps(archive(args.directory, args.name)))
        return
    count = 0
    for path in sorted(args.directory.glob('*.json')):
        stub = json.loads(path.read_text())
        if stub.get('archive_schema') != SCHEMA: continue
        data = read_bytes(path)
        if args.action == 'extract':
            if not args.output: parser.error('extract requires --output')
            args.output.mkdir(parents=True, exist_ok=True)
            target = args.output / path.name
            if target.exists() and target.read_bytes() != data:
                raise ValueError('refusing to overwrite different evidence: ' + str(target))
            target.write_bytes(data)
        count += 1
    print(f'measurement archive: {count} original receipts verified')


if __name__ == '__main__':
    main()
