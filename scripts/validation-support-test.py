#!/usr/bin/env python3
"""Behavioral checks for execution, evidence reuse and installer ownership."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from validation_support import ROOT, PlanRunner, command, save_json, audit_steps


class ValidationTests(unittest.TestCase):
    def test_failed_and_changed_artifact_evidence_is_not_reused(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, artifact, count = (root / n for n in ('source', 'artifact', 'count'))
            source.write_text('one')
            program = ('from pathlib import Path; '
                       f'p=Path({str(count)!r}); p.write_text(p.read_text()+"x" if p.exists() else "x"); '
                       f'Path({str(artifact)!r}).write_text("built")')
            argv = [sys.executable, '-c', program]
            def run():
                runner = PlanRunner(root / 'evidence', deadline=30)
                runner.run('case', argv, inputs=(str(source),), artifacts=(str(artifact),))
                return runner
            run(); run()
            self.assertEqual(count.read_text(), 'x')
            artifact.write_text('corrupted'); run()
            self.assertEqual(count.read_text(), 'xx')
            source.write_text('two'); verified=run()
            audit_steps(verified.rows,['case'])
            artifact.write_text('modified after step')
            with self.assertRaisesRegex(ValueError,'outputs changed'): audit_steps(verified.rows,['case'])
            self.assertEqual(count.read_text(), 'xxx')
            runner = PlanRunner(root / 'evidence', deadline=30)
            for _ in range(2):
                with self.assertRaises(RuntimeError):
                    runner.run('failure', [sys.executable, '-c', 'raise SystemExit(7)'], inputs=(str(source),))
                self.assertEqual(runner.rows['failure']['status'], 'failed')

    def test_timeout_stops_descendants(self):
        with tempfile.TemporaryDirectory() as directory:
            marker = Path(directory) / 'orphan'
            child = f'import time; from pathlib import Path; time.sleep(1); Path({str(marker)!r}).touch()'
            parent = f'import subprocess,sys,time; subprocess.Popen([sys.executable,"-c",{child!r}]); time.sleep(20)'
            with self.assertRaises(subprocess.TimeoutExpired):
                command([sys.executable, '-c', parent], timeout=.15,
                        env={k: v for k, v in os.environ.items() if k != 'BIRNAM_VALIDATION_CHILD'})
            command([sys.executable, '-c', 'import time; time.sleep(1.1)'])
            self.assertFalse(marker.exists())

    def test_atomic_save_preserves_previous_receipt_on_serialization_error(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'receipt.json'
            save_json(path, {'status': 'passed'})
            with self.assertRaises(TypeError): save_json(path, {'bad': object()})
            self.assertEqual(json.loads(path.read_text()), {'status': 'passed'})
            self.assertEqual(len(list(path.parent.iterdir())), 1)

    def test_plans_have_explicit_scope_and_complete_prerequisites(self):
        from validate import plan
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            routine=plan('routine',root/'prefix',root/'output')
            release=plan('release',root/'prefix',root/'output')
            for steps in (routine,release):
                seen=set()
                for step in steps:
                    self.assertNotIn(step['name'],seen)
                    self.assertTrue(set(step.get('depends',())) <= seen)
                    seen.add(step['name'])
            self.assertEqual(sum(s['name']=='workspace' for s in release),1)
            self.assertEqual(sum(s['name']=='cold-source' for s in release),1)
            self.assertFalse(any(s['name']=='cold-source' for s in routine))
            self.assertEqual({s['name'] for s in routine if s['name'].startswith('diagnostics-')},{'diagnostics-release'})
            self.assertEqual({s['name'] for s in release if s['name'].startswith('diagnostics-')},{'diagnostics-debug','diagnostics-release'})
            for steps in (routine,release):
                sequences=[s for s in steps if 'scripts/closure-sequence-smoke.py' in s['argv']]
                self.assertEqual(len(sequences),4 if steps is routine else 8)
                self.assertTrue(all('--quick' not in s['argv'] for s in steps))
            self.assertIn('--all-families',next(s for s in release if s['name']=='recovery')['argv'])

    def test_ci_shards_cover_each_matrix_once(self):
        import importlib.util
        spec=importlib.util.spec_from_file_location('ci',ROOT/'scripts/validation-ci.py')
        ci=importlib.util.module_from_spec(spec);spec.loader.exec_module(ci)
        for suite in ('exhaustive','release'):
            steps,core,matrices=ci.groups(suite)
            names=[name for shard in ci.shards(matrices) for name in shard]
            self.assertEqual(len(names),len(set(names)))
            self.assertEqual(set(names),{s['name'] for s in matrices})
            self.assertEqual(set(core)|set(names)|{'complete-matrix-oracle'},{s['name'] for s in steps})
        self.assertEqual(ci.select(['docs/validation.md']),'routine')
        self.assertEqual(ci.select(['rust/birnam-id/src/runtime.rs']),'exhaustive')
        self.assertEqual(ci.select(['scripts/rust-install.sh']),'release')

    def test_ci_seed_preserves_artifact_bytes_and_executable_mode(self):
        import importlib.util
        import shutil
        spec=importlib.util.spec_from_file_location('ci_seed',ROOT/'scripts/validation-ci.py')
        ci=importlib.util.module_from_spec(spec);spec.loader.exec_module(ci)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            ci.ROOT=root;ci.PREFIX=root/'published/prefix';ci.OUTPUT=root/'evidence';ci.TARGET=root/'diagnostics'
            binary=ci.PREFIX/'bin/birnam';binary.parent.mkdir(parents=True);binary.write_bytes(b'executable');binary.chmod(0o755)
            ci.OUTPUT.mkdir();(ci.OUTPUT/'proof.json').write_text('{"status":"passed"}')
            for profile in ('debug','release'):
                (ci.TARGET/profile).mkdir(parents=True)
                for name in ci.LIBRARIES:(ci.TARGET/profile/name).write_text(profile+name)
            archive=root/'seed.tgz';ci.pack_seed(archive)
            shutil.rmtree(ci.PREFIX);shutil.rmtree(ci.OUTPUT);shutil.rmtree(ci.TARGET)
            ci.unpack_seed(archive)
            self.assertEqual(binary.read_bytes(),b'executable')
            self.assertEqual(binary.stat().st_mode & 0o777,0o755)
            for profile in ('debug','release'):
                for name in ci.LIBRARIES:self.assertEqual((ci.TARGET/profile/name).read_text(),profile+name)
            self.assertEqual(json.loads((ci.OUTPUT/'proof.json').read_text())['status'],'passed')

    def test_installer_cleanup_and_status(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cargo = root / 'cargo'
            cargo.write_text('#!/bin/sh\nprintf "%s" "$CARGO_TARGET_DIR" > "$PROBE"\nexit "$CODE"\n')
            cargo.chmod(0o755)
            env = dict(os.environ, PATH=str(root)+':'+os.environ['PATH'], TMPDIR=directory, PROBE=str(root/'probe'))
            for code in (0, 17):
                process = subprocess.run(['bash', str(ROOT/'scripts/rust-install.sh'), 'install'], env=dict(env, CODE=str(code)))
                self.assertEqual(process.returncode, code)
                self.assertFalse(Path((root/'probe').read_text()).exists())
            process = subprocess.run(['bash', str(ROOT/'scripts/rust-install.sh'), 'install', '--build-cache', str(root/'cache')], env=dict(env, CODE='0'))
            self.assertEqual(process.returncode, 0)
            self.assertTrue(Path((root/'probe').read_text()).is_dir())


if __name__ == '__main__':
    unittest.main()
