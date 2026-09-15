"""Lossless historical receipts. A checked-in stub binds exact archived bytes."""
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import zipfile

SCHEMA = 'birnam-measurement-archive-v1'


@lru_cache(maxsize=4)
def _archive(path, modified, size):
    # Retain only the directory index; ZipFile reads payloads on demand. Cache
    # lifetime is one CLI process, and path/stat changes select a fresh reader.
    return zipfile.ZipFile(path)


def read_bytes(path):
    path = Path(path)
    raw = path.read_bytes()
    if path.suffix != '.json' or SCHEMA.encode() not in raw[:256]:
        return raw
    stub = json.loads(raw)
    if not isinstance(stub, dict) or stub.get('archive_schema') != SCHEMA:
        return raw
    archive_name, member = stub['archive'], stub['member']
    if Path(archive_name).name != archive_name or Path(member).is_absolute() or '..' in Path(member).parts:
        raise ValueError('invalid archived receipt location')
    archive = path.parent / archive_name
    stat = archive.stat()
    data = _archive(str(archive.resolve()), stat.st_mtime_ns, stat.st_size).read(member)
    if len(data) != stub['bytes'] or hashlib.sha256(data).hexdigest() != stub['sha256']:
        raise ValueError('archived receipt hash/size mismatch: ' + str(path))
    return data


def load_json(path):
    return json.loads(read_bytes(path))
