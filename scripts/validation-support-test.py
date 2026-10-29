#!/usr/bin/env python3
"""Behavioral checks for execution, evidence reuse and installer ownership."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import signal
from validation_support import ROOT, PlanRunner, command, save_json, audit_steps, audit_plan


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

    def test_dead_group_signal_race_preserves_timeout(self):
        actual_signal = os.killpg
        actual_ps = subprocess.check_output
        for still_live in [False, True]:
            with self.subTest(still_live=still_live):
                group = []; observations = []
                def send(pgid, sig):
                    group[:] = [pgid]
                    if sig == signal.SIGKILL:
                        raise PermissionError('simulated Darwin group race')
                    return actual_signal(pgid, sig)
                def snapshot(argv, **kwargs):
                    if argv != ['ps', '-axo', 'pgid=,stat=']:
                        return actual_ps(argv, **kwargs)
                    observations.append(True)
                    return f'{group[0]} R\n' if len(observations) == 1 or still_live else ''
                with patch('validation_support.os.killpg', side_effect=send), patch('validation_support.subprocess.check_output', side_effect=snapshot):
                    with self.assertRaises(PermissionError if still_live else subprocess.TimeoutExpired):
                        command([sys.executable, '-c', 'import time; time.sleep(20)'],
                                timeout=.15, env={'PATH': os.environ['PATH']})
                self.assertEqual(len(observations), 2)

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
            for steps in (routine, release):
                native = next(s for s in steps if s['name'] == 'native')
                self.assertEqual(native['argv'], ['bash', 'scripts/native-message-test.sh'])
                hosts = [s for s in steps if s['name'].startswith('host-') and s['name'] != 'host-test-binaries']
                self.assertEqual({s['name'] for s in hosts},
                                 {'host-cli', 'host-image', 'host-interaction', 'host-generation', 'host-tasks'})
                self.assertTrue(all(s['depends'] == ('install', 'host-test-binaries') for s in hosts))
                prepared = next(s for s in steps if s['name'] == 'host-test-binaries')
                self.assertIn('scripts/**/*.birnam', prepared['inputs'])
                self.assertIn('tests/**/*', prepared['inputs'])
                self.assertTrue(all('scripts/**/*.birnam' in s['inputs'] for s in hosts))
                self.assertNotIn('birnam-runtime', json.dumps(steps))

    def test_plan_audit_rejects_receipts_for_another_command_or_dependency(self):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'source'
            source.write_text('input')
            step = dict(name='case', argv=[sys.executable, '-c', 'print("accepted")'],
                        inputs=(str(source),))
            runner = PlanRunner(Path(directory) / 'evidence', deadline=30)
            runner.run(**step)
            audit_plan(runner.rows, [step])
            for changed, message in [
                (dict(argv=[sys.executable, '-c', 'raise SystemExit(1)']), 'command'),
                (dict(depends=('unexecuted',)), 'prerequisites'),
                (dict(inputs=(str(source), 'Cargo.toml')), 'inputs'),
                (dict(artifacts=(str(source),)), 'outputs'),
            ]:
                with self.subTest(changed=changed):
                    with self.assertRaisesRegex(ValueError, message):
                        audit_plan(runner.rows, [dict(step, **changed)])
            with self.assertRaisesRegex(ValueError, 'command'):
                audit_plan({}, [step])

    def test_ci_shards_cover_each_matrix_once(self):
        import importlib.util
        spec=importlib.util.spec_from_file_location('ci',ROOT/'scripts/validation-ci.py')
        ci=importlib.util.module_from_spec(spec);spec.loader.exec_module(ci)
        for suite in ('exhaustive','release'):
            steps,core,matrices=ci.groups(suite)
            names=[name for shard in ci.shards(matrices) for name in shard]
            self.assertEqual(len(names),len(set(names)))
            self.assertEqual(set(names),{s['name'] for s in matrices})
            self.assertEqual(set(core)|set(names),{s['name'] for s in steps})
        self.assertEqual(ci.select(['docs/validation.md']),'routine')
        self.assertEqual(ci.select(['rust/birnam-execution/src/lib.rs']),'exhaustive')
        for path in ('scripts/rust-install.sh', 'scripts/dist.sh', 'scripts/release.sh',
                     'scripts/install-smoke.sh', 'scripts/install_contract.py', 'scripts/release_archive.py'):
            self.assertEqual(ci.select([path]), 'release')

    def test_ci_seed_preserves_artifact_bytes_and_executable_mode(self):
        import importlib.util
        import shutil
        spec=importlib.util.spec_from_file_location('ci_seed',ROOT/'scripts/validation-ci.py')
        ci=importlib.util.module_from_spec(spec);spec.loader.exec_module(ci)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            ci.ROOT=root;ci.PREFIX=root/'published/prefix';ci.OUTPUT=root/'evidence'
            binary=ci.PREFIX/'bin/birnam';binary.parent.mkdir(parents=True);binary.write_bytes(b'executable');binary.chmod(0o755)
            ci.OUTPUT.mkdir();(ci.OUTPUT/'proof.json').write_text('{"status":"passed"}')
            archive=root/'seed.tgz';ci.pack_seed(archive)
            shutil.rmtree(ci.PREFIX);shutil.rmtree(ci.OUTPUT)
            ci.unpack_seed(archive)
            self.assertEqual(binary.read_bytes(),b'executable')
            self.assertEqual(binary.stat().st_mode & 0o777,0o755)
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


class HostLanes(unittest.TestCase):
    def test_two_lanes_overlap_and_all_receipts_survive(self):
        from validate import run_plan, HOST_GROUPS
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); source=root/'source';source.write_text('same inputs')
            runner=PlanRunner(root/'evidence',deadline=20)
            first=root/'first';second=root/'second'
            steps=[]
            for name, own, peer in [('host-interaction',first,second),('host-tasks',second,first)]:
                code=('from pathlib import Path; import time; '
                      f'Path({str(own)!r}).touch(); p=Path({str(peer)!r}); '
                      'deadline=time.monotonic()+5\n'
                      'while not p.exists():\n'
                      ' assert time.monotonic()<deadline,"other lane never started"\n'
                      ' time.sleep(.01)\n')
                steps.append(dict(name=name,argv=[sys.executable,'-c',code],inputs=(str(source),)))
            for name in ('cli','image','generation'):
                steps.append(dict(name='host-'+name,argv=[sys.executable,'-c','print("checked")'],inputs=(str(source),)))
            run_plan(runner,steps,2)
            audit_plan(runner.rows,steps)
            self.assertEqual(set(runner.rows),{'host-'+name for name in HOST_GROUPS})
            saved=json.loads((root/'evidence/run.json').read_text())
            self.assertEqual(set(saved['steps']),set(runner.rows))
            self.assertTrue(all(row['status']=='passed' for row in saved['steps'].values()))

    def test_lane_failure_cancels_peer_descendants_and_never_starts_followup(self):
        from validate import run_plan
        import time
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); source=root/'source';source.write_text('input')
            runner=PlanRunner(root/'evidence',deadline=10)
            # Exercise the top-level qualification process-group ownership.
            runner.env.pop('BIRNAM_VALIDATION_CHILD',None)
            started=root/'started';orphan=root/'orphan';unstarted=root/'unstarted'
            child=f'import time; from pathlib import Path; time.sleep(1); Path({str(orphan)!r}).touch()'
            slow=('import subprocess,sys,time; from pathlib import Path; '
                  f'subprocess.Popen([sys.executable,"-c",{child!r}]); Path({str(started)!r}).touch(); time.sleep(20)')
            failed=('from pathlib import Path; import time; '+f'p=Path({str(started)!r})\n'
                    'while not p.exists(): time.sleep(.01)\nraise SystemExit(7)')
            steps=[dict(name='host-interaction',argv=[sys.executable,'-c',failed],inputs=(str(source),)),
                   dict(name='host-tasks',argv=[sys.executable,'-c',slow],inputs=(str(source),)),
                   dict(name='host-cli',argv=[sys.executable,'-c',f'from pathlib import Path; Path({str(unstarted)!r}).touch()'],inputs=(str(source),))]
            with self.assertRaises(RuntimeError): run_plan(runner,steps,2)
            time.sleep(1.1)
            self.assertTrue(started.exists())
            self.assertEqual(runner.rows['host-interaction']['exit_code'],7)
            self.assertFalse(orphan.exists())
            self.assertFalse(unstarted.exists())
            self.assertEqual(runner.rows['host-interaction']['status'],'failed')
            self.assertEqual(runner.rows['host-tasks']['status'],'failed')

    def test_snapshot_survives_cargo_output_replacement_and_rejects_tampering(self):
        import host_test_binaries as host
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);original=root/'cargo-output';original.write_text('#!/bin/sh\nexit 0\n');original.chmod(0o755)
            def cargo(argv,**kwargs):
                key=argv[argv.index('-p')+1]
                target=argv[argv.index('--test')+1] if '--test' in argv else key.replace('-','_')
                return subprocess.CompletedProcess(argv,0,stdout=json.dumps(dict(reason='compiler-artifact',
                    executable=str(original),profile=dict(test=True),target=dict(name=target)))+'\n')
            with patch.object(host.subprocess,'run',side_effect=cargo): host.prepare(root/'frozen')
            manifest=root/'frozen/manifest.json'
            frozen=host.executable(manifest,'host-lib')
            original.write_text('new Cargo output')
            self.assertEqual(frozen.read_text(),'#!/bin/sh\nexit 0\n')
            frozen.write_text('tampered snapshot')
            with self.assertRaisesRegex(ValueError,'changed'): host.executable(manifest,'host-lib')

    def test_timing_records_success_and_failure_exit_status(self):
        from validation_timed import run
        with tempfile.TemporaryDirectory() as temporary:
            output=Path(temporary)/'timings.json'
            self.assertEqual(run(output,'success',[sys.executable,'-c','print("ok")']),0)
            self.assertEqual(run(output,'failure',[sys.executable,'-c','raise SystemExit(9)']),9)
            rows=json.loads(output.read_text())['checks']
            self.assertEqual([r['status'] for r in rows],['passed','failed'])
            self.assertEqual([r['exit_code'] for r in rows],[0,9])
            self.assertTrue(all(r['wall_ns']>0 for r in rows))


if __name__ == '__main__':
    unittest.main()
