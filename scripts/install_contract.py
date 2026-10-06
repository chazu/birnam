"""Validate a published prefix against its current Rust installation receipt."""
import hashlib
import json
import os
from pathlib import Path
import sys

RECEIPT = 'share/birnam/rust-install.json'


def verify(prefix):
    prefix = Path(prefix).resolve()
    receipt = json.loads((prefix / RECEIPT).read_text())
    if receipt.get('schema') != 4 or receipt.get('authority') != 'rust':
        raise ValueError('expected a schema-4 Rust installation receipt')
    contract = receipt['admission_contract']
    if (contract['target'], contract['id_protocol_version'], contract['stdlib_loading']) != (
            'aarch64-macos', 1, 'compiled-image'):
        raise ValueError('unsupported installation admission contract')
    expected = receipt['staged_manifest']
    actual = {}
    for path in sorted(prefix.rglob('*')):
        relative = path.relative_to(prefix).as_posix()
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError('unsupported installed file: ' + relative)
        if path.is_file() and relative != RECEIPT:
            actual[relative] = hashlib.sha256(path.read_bytes()).hexdigest()
    if actual != expected:
        changed = sorted(k for k in actual.keys() | expected.keys()
                         if actual.get(k) != expected.get(k))
        raise ValueError('installed manifest differs: ' + ', '.join(changed))
    encoded = json.dumps(actual, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()
    if (len(actual) != receipt['staged_manifest_files'] or
            hashlib.sha256(encoded).hexdigest() != receipt['staged_manifest_sha256']):
        raise ValueError('installed manifest count or digest differs')
    required = {'lib/birnam-bootstrap.image', 'lib/birnam-tools.image', 'bin/birnam', 'share/birnam/src/Cargo.toml', 'share/birnam/src/Cargo.lock'}
    required.update('lib/' + name for name in receipt['dynamic_libraries'])
    required.update('lib/libbirnam_' + name + '.dylib' for name in receipt.get('bootstrap_modules', []))
    required.update('lib/libbirnam_' + name + '.dylib' for name in receipt.get('optional_modules', []))
    if (prefix / 'lib/libbirnam_lang_seed.dylib').exists():
        raise ValueError('build-only Lang seed is present in the installation')
    if not required <= actual.keys():
        raise ValueError('installation is missing required artifacts')
    if not os.access(prefix / 'bin/birnam', os.X_OK):
        raise ValueError('installed host is not executable')
    return receipt


if __name__ == '__main__':
    verify(sys.argv[1])
    print('installed contract: exact file closure, bytes, receipt and executable passed')
