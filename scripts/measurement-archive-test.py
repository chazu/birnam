#!/usr/bin/env python3
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from measurement_archive import read_bytes, load_json

spec = importlib.util.spec_from_file_location('archiver', Path(__file__).with_name('measurement-archive.py'))
archiver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archiver)


class ArchiveTests(unittest.TestCase):
    def test_exact_bytes_and_corruption_detection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / 'old.json'
            raw = b'{ "status":"passed", "nested": [1, 2] }\n\n'
            path.write_bytes(raw)
            archiver.archive(root, 'history.zip')
            self.assertEqual(read_bytes(path), raw)
            self.assertEqual(load_json(path)['nested'], [1, 2])
            stub = json.loads(path.read_text())
            stub['sha256'] = '0' * 64
            path.write_text(json.dumps(stub))
            with self.assertRaisesRegex(ValueError, 'hash/size mismatch'): read_bytes(path)
            stub['archive'] = '../history.zip'
            path.write_text(json.dumps(stub))
            with self.assertRaisesRegex(ValueError, 'location'): read_bytes(path)


if __name__ == '__main__': unittest.main()
