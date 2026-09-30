#!/usr/bin/env python3
"""Receipt integrity must reject drift in a relocated release closure."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from install_contract import RECEIPT, verify


class InstalledContract(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.prefix = Path(self.directory.name) / 'relocated'
        for name in ('bin/birnam', 'lib/libbirnam_id.dylib',
                     'share/birnam/src/Cargo.toml', 'share/birnam/src/Cargo.lock'):
            path = self.prefix / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(name.encode())
        (self.prefix / 'bin/birnam').chmod(0o755)
        self.receipt = dict(schema=4, authority='rust',
                            admission_contract=dict(target='aarch64-macos', id_protocol_version=1,
                                                    stdlib_loading='source'),
                            dynamic_libraries=['libbirnam_id.dylib'])
        self.publish()

    def publish(self):
        manifest = {p.relative_to(self.prefix).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in sorted(self.prefix.rglob('*'))
                    if p.is_file() and p.relative_to(self.prefix).as_posix() != RECEIPT}
        encoded = json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()
        self.receipt.update(staged_manifest=manifest, staged_manifest_files=len(manifest),
                            staged_manifest_sha256=hashlib.sha256(encoded).hexdigest())
        (self.prefix / RECEIPT).write_text(json.dumps(self.receipt))

    def test_exact_relocated_closure(self):
        self.assertEqual(verify(self.prefix), self.receipt)

    def test_rejects_modified_removed_and_unlisted_files(self):
        library = self.prefix / 'lib/libbirnam_id.dylib'
        original = library.read_bytes()
        library.write_bytes(b'different signing bytes')
        with self.assertRaisesRegex(ValueError, 'manifest differs'): verify(self.prefix)
        library.unlink()
        with self.assertRaisesRegex(ValueError, 'manifest differs'): verify(self.prefix)
        library.write_bytes(original)
        (self.prefix / 'unlisted').write_bytes(b'extra')
        with self.assertRaisesRegex(ValueError, 'manifest differs'): verify(self.prefix)

    def test_rejects_symlinks_and_missing_executable_mode(self):
        (self.prefix / 'external').symlink_to(self.prefix.parent, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'unsupported installed file'): verify(self.prefix)
        (self.prefix / 'external').unlink()
        (self.prefix / 'bin/birnam').chmod(0o644)
        with self.assertRaisesRegex(ValueError, 'not executable'): verify(self.prefix)

    def test_rejects_wrong_receipt_and_incomplete_declared_closure(self):
        self.receipt['schema'] = 3
        self.publish()
        with self.assertRaisesRegex(ValueError, 'schema-4'): verify(self.prefix)
        self.receipt['schema'] = 4
        (self.prefix / 'lib/libbirnam_id.dylib').unlink()
        self.publish()
        with self.assertRaisesRegex(ValueError, 'required artifacts'): verify(self.prefix)

    def test_rejects_manifest_summary_drift(self):
        for field, value in [('staged_manifest_files', 0), ('staged_manifest_sha256', 'bad')]:
            self.publish()
            self.receipt[field] = value
            (self.prefix / RECEIPT).write_text(json.dumps(self.receipt))
            with self.assertRaisesRegex(ValueError, 'count or digest'): verify(self.prefix)


if __name__ == '__main__':
    unittest.main()
