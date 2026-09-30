#!/usr/bin/env python3
"""Check candidate extraction before any executable from an archive is run."""
import importlib.util
import io
import hashlib
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('acceptance', Path(__file__).with_name('release-acceptance.py'))
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


class CandidateExtraction(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.archive = self.root / 'candidate.tar.gz'
        self.destination = self.root / 'unpacked'
        self.destination.mkdir()

    def package(self, members):
        with tarfile.open(self.archive, 'w:gz') as archive:
            for name, kind in members:
                member = tarfile.TarInfo(name)
                member.type = kind
                member.mode = 0o755
                data = b'candidate'
                if kind == tarfile.REGTYPE:
                    member.size = len(data)
                if kind in (tarfile.LNKTYPE, tarfile.SYMTYPE):
                    member.linkname = '../../outside'
                archive.addfile(member, io.BytesIO(data) if kind == tarfile.REGTYPE else None)

    def test_exact_prefix_and_modes_survive(self):
        self.package([('birnam/', tarfile.DIRTYPE), ('birnam/bin/', tarfile.DIRTYPE),
                      ('birnam/bin/birnam', tarfile.REGTYPE)])
        prefix = acceptance.extract_candidate(self.archive, self.destination)
        self.assertEqual(prefix, self.destination / 'birnam')
        self.assertEqual((prefix / 'bin/birnam').read_bytes(), b'candidate')
        self.assertTrue((prefix / 'bin/birnam').stat().st_mode & 0o111)

    def test_rejects_links_special_files_and_path_escape_before_extracting(self):
        for name, kind in [('birnam/escape', tarfile.SYMTYPE), ('birnam/link', tarfile.LNKTYPE),
                           ('birnam/pipe', tarfile.FIFOTYPE), ('../outside', tarfile.REGTYPE),
                           ('/outside', tarfile.REGTYPE), ('birnam/../outside', tarfile.REGTYPE),
                           ('birnam//overlaid', tarfile.REGTYPE)]:
            with self.subTest(name=name, kind=kind):
                self.package([('birnam/bin/birnam', tarfile.REGTYPE), (name, kind)])
                with self.assertRaisesRegex(ValueError, 'invalid archive member'):
                    acceptance.extract_candidate(self.archive, self.destination)
                self.assertEqual(list(self.destination.iterdir()), [])

    def test_rejects_duplicate_entries_and_multiple_roots(self):
        for members, diagnostic in [
            ([('birnam/bin/birnam', tarfile.REGTYPE)] * 2, 'duplicate'),
            ([('birnam/bin/birnam', tarfile.REGTYPE), ('other', tarfile.REGTYPE)], 'exactly one'),
            ([], 'exactly one'),
            ([('birnam', tarfile.REGTYPE)], 'not a directory'),
        ]:
            with self.subTest(members=members):
                self.package(members)
                with self.assertRaisesRegex(ValueError, diagnostic):
                    acceptance.extract_candidate(self.archive, self.destination)


class CandidateIdentity(unittest.TestCase):
    def test_rejection_records_failure_before_starting_any_gate(self):
        for identity, mode, diagnostic in [('another-commit', 'cold', 'different source commits'),
                                            ('checkout', 'development-cache', 'cold release')]:
            with self.subTest(identity=identity, mode=mode), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                prefix = root / 'prefix'
                for relative in ('bin/birnam', 'share/birnam/src/Cargo.toml', 'share/birnam/src/Cargo.lock'):
                    path = prefix / relative
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(b'must never execute')
                (prefix / 'bin/birnam').chmod(0o755)
                manifest = {p.relative_to(prefix).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                            for p in prefix.rglob('*') if p.is_file()}
                encoded = json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()
                receipt = dict(schema=4, authority='rust', profile='release', build_mode=mode,
                    source_identity=dict(git_commit=identity, git_clean=True), dynamic_libraries=[],
                    admission_contract=dict(target='aarch64-macos', id_protocol_version=1, stdlib_loading='source'),
                    staged_manifest=manifest, staged_manifest_files=len(manifest),
                    staged_manifest_sha256=hashlib.sha256(encoded).hexdigest())
                (prefix / acceptance.RECEIPT).write_text(json.dumps(receipt))
                archive = root / 'candidate.tar.gz'
                with tarfile.open(archive, 'w:gz') as package:
                    package.add(prefix, arcname='prefix')
                output = root / 'evidence'
                with patch.object(acceptance, 'git', side_effect=lambda *args: 'checkout' if args[0] == 'rev-parse' else ''), \
                     patch.object(acceptance, 'PlanRunner') as runner:
                    with self.assertRaisesRegex(ValueError, diagnostic):
                        acceptance.accept(archive, output)
                    runner.assert_not_called()
                evidence = json.loads((output / 'receipt.json').read_text())
                self.assertEqual(evidence['result'], 'failed')
                self.assertFalse(evidence['promotable'])
                self.assertIn(diagnostic, evidence['error'])


if __name__ == '__main__':
    unittest.main()
