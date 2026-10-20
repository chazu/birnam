#!/usr/bin/env python3
"""Release orchestration: exact evidence, interruption recovery, and no duplicate build."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import validation_support as support
from install_contract import RECEIPT, verify
from validate import plan

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('acceptance', ROOT/'scripts/release-acceptance.py')
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)


def prefix_fixture(prefix, *, mode='cold', commit='checkout', version='0.1.1'):
    for relative, content in {
        'bin/birnam': b'not executed by receipt tests',
        'lib/birnam-bootstrap.image': b'BIRNAMIMbootstrap',
        'lib/birnam-tools.image': b'BIRNAMIMtools',
        'share/birnam/src/Cargo.toml': b'package',
        'share/birnam/src/Cargo.lock': b'lock',
        'share/birnam/src/VERSION': version.encode(),
    }.items():
        path = prefix/relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    (prefix/'bin/birnam').chmod(0o755)
    manifest = {p.relative_to(prefix).as_posix(): support.sha(p)
                for p in prefix.rglob('*') if p.is_file() and p.relative_to(prefix).as_posix() != RECEIPT}
    receipt = dict(schema=4, authority='rust', profile='release', build_mode=mode,
        source_identity=dict(git_commit=commit, git_clean=True), dynamic_libraries=[],
        source_manifest_sha256='source', toolchain={'rustc': 'fixture'},
        admission_contract=dict(target='aarch64-macos', id_protocol_version=1, stdlib_loading='compiled-image'),
        staged_manifest=manifest, staged_manifest_files=len(manifest),
        staged_manifest_sha256=hashlib.sha256(json.dumps(manifest, sort_keys=True, separators=(',', ':')).encode()).hexdigest())
    support.save_json(prefix/RECEIPT, receipt)
    return receipt


class Qualification(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.prefix = self.root/'prefix'
        self.install = prefix_fixture(self.prefix)
        self.source = self.root/'input'; self.source.write_text('original')
        self.steps = [dict(name='install', argv=[sys.executable, '-c', 'print("fixture install")'],
                          inputs=(str(self.source),), artifacts=(str(self.prefix),)),
                      dict(name='host-cli', argv=[sys.executable, '-c', 'print("fixture host")'],
                          inputs=(str(self.source),), depends=('install',))]
        self.runner = support.PlanRunner(self.root/'qualification', deadline=30)
        for step in self.steps: self.runner.run(**step)
        self.runner.receipt.update(status='passed', suite='release', candidate=True,
            prefix=str(self.prefix), source=self.install['source_identity'],
            selected_steps=[s['name'] for s in self.steps])
        self.runner.save()
        self.receipt = self.root/'qualification/run.json'
        self.archive = self.root/'candidate.tar.gz'
        with tarfile.open(self.archive, 'w:gz') as package: package.add(self.prefix, arcname='candidate')
        self.plan_patch = patch.object(acceptance, 'plan', return_value=self.steps)
        self.plan_patch.start(); self.addCleanup(self.plan_patch.stop)

    def check(self):
        return acceptance.check_qualification(self.receipt, self.install, self.runner)

    def test_complete_matching_receipts_are_accepted(self):
        self.assertEqual(self.check(), support.sha(self.receipt))

    def test_missing_failed_warm_or_different_artifact_proof_is_rejected(self):
        original = json.loads(self.receipt.read_text())
        for change in [dict(status='failed'), dict(candidate=False), dict(selected_steps=['install']),
                       dict(source={'git_commit': 'different', 'git_clean': True}), dict(toolchain={})]:
            with self.subTest(change=change):
                support.save_json(self.receipt, dict(original, **change))
                with self.assertRaises(ValueError): self.check()
        support.save_json(self.receipt, original)
        other = dict(self.install, staged_manifest_sha256='other archive')
        with self.assertRaisesRegex(ValueError, 'different candidates'):
            acceptance.check_qualification(self.receipt, other, self.runner)
        (self.prefix/'bin/birnam').write_bytes(b'tampered')
        with self.assertRaisesRegex(ValueError, 'manifest differs'): self.check()

    def test_current_inputs_logs_and_required_commands_are_reaudited(self):
        self.source.write_text('changed')
        with self.assertRaisesRegex(ValueError, 'inputs changed'): self.check()
        self.source.write_text('original')
        changed = dict(self.steps[1], argv=[sys.executable, '-c', 'print("another test")'])
        with patch.object(acceptance, 'plan', return_value=[self.steps[0], changed]):
            with self.assertRaisesRegex(ValueError, 'command differs'): self.check()
        Path(self.runner.rows['host-cli']['log']).write_text('tampered')
        with self.assertRaisesRegex(ValueError, 'log changed'): self.check()

    def test_failed_archive_step_retries_then_success_resumes(self):
        calls = []
        fail = [True]
        actual_command = support.command
        def command(argv, **kwargs):
            if 'scripts/release_archive.py' in argv:
                calls.append(list(argv))
                code = 7 if fail[0] else 0
                result = actual_command([sys.executable, '-c', f'raise SystemExit({code})'], **kwargs)
                result['argv'] = list(argv)
                return result
            return actual_command(argv, **kwargs)
        with patch.object(support, 'command', side_effect=command), \
             patch.object(acceptance, 'git', side_effect=lambda *a: 'checkout' if a[0]=='rev-parse' else ''):
            output = self.root/'acceptance'
            with self.assertRaises(RuntimeError):
                acceptance.accept(self.archive, output, qualification=self.receipt)
            self.assertFalse(json.loads((output/'receipt.json').read_text())['promotable'])
            fail[0] = False
            acceptance.accept(self.archive, output, qualification=self.receipt)
            # A resumed qualifier changes only display flags, not gate proofs.
            qualified = json.loads(self.receipt.read_text())
            for row in qualified['steps'].values(): row['reused'] = True
            support.save_json(self.receipt, qualified)
            acceptance.accept(self.archive, output, qualification=self.receipt)
            self.assertEqual(len(calls), 2)
            result = json.loads((output/'receipt.json').read_text())
            self.assertTrue(result['promotable'])
            self.assertEqual(result['qualification']['sha256'], support.sha(self.receipt))
            self.assertEqual(json.loads((output/'gates/run.json').read_text())['selected_steps'], ['archive-smoke'])
            # A later source edit invalidates the old qualification before even
            # a cached archive success can be reused.
            self.source.write_text('changed')
            with self.assertRaisesRegex(ValueError, 'inputs changed'):
                acceptance.accept(self.archive, output, qualification=self.receipt)
            self.assertEqual(len(calls), 2)
            self.assertFalse(json.loads((output/'receipt.json').read_text())['promotable'])

    def test_mutation_during_smoke_cannot_be_promoted(self):
        actual_command = support.command
        def command(argv, **kwargs):
            if 'scripts/release_archive.py' in argv:
                self.source.write_text('changed during check')
                result = actual_command([sys.executable, '-c', 'print("smoke passed")'], **kwargs)
                result['argv'] = list(argv)
                return result
            return actual_command(argv, **kwargs)
        with patch.object(support, 'command', side_effect=command), \
             patch.object(acceptance, 'git', side_effect=lambda *a: 'checkout' if a[0]=='rev-parse' else ''):
            with self.assertRaisesRegex(ValueError, 'inputs changed'):
                acceptance.accept(self.archive, self.root/'acceptance', qualification=self.receipt)
        self.assertFalse(json.loads((self.root/'acceptance/receipt.json').read_text())['promotable'])

    def test_candidate_plan_keeps_all_gates_and_has_one_cold_shipping_build(self):
        ordinary = plan('release', self.prefix, self.root)
        candidate = plan('release', self.prefix, self.root, candidate=True)
        self.assertEqual([s['name'] for s in candidate], [s['name'] for s in ordinary])
        build = [s for s in candidate if s['name']=='install']
        self.assertEqual(len(build), 1)
        self.assertNotIn('--build-cache', build[0]['argv'])
        self.assertIn('--build-cache', next(s for s in ordinary if s['name']=='install')['argv'])
        self.assertIn(str(self.root/'candidate-source.json'), build[0]['inputs'])
        self.assertEqual(len([s for s in candidate if s['name'].startswith('host-') and s['name'] != 'host-test-binaries']), 5)
        self.assertIn('cold-source', [s['name'] for s in candidate])
        with self.assertRaises(ValueError): plan('routine', self.prefix, self.root, candidate=True)


class CandidateResume(unittest.TestCase):
    def test_real_candidate_runner_reuses_build_and_invalidates_new_commit(self):
        import validate
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); prefix = root/'prefix'; output = root/'qualification'
            base = prefix_fixture(prefix)
            build_count = root/'build-count'; host_count = root/'host-count'
            # A tiny executable build fixture exercises the actual candidate
            # runner, cache, receipt audit and final identity check without Cargo.
            builder = root/'build.py'
            builder.write_text("""import json
from pathlib import Path
source=Path(SOURCE)
receipt=Path(RECEIPT_PATH)
count=Path(COUNT)
count.write_text(count.read_text()+'x' if count.exists() else 'x')
data=json.loads(receipt.read_text())
data['source_identity']=json.loads(source.read_text())
receipt.write_text(json.dumps(data))
""".replace('SOURCE',repr(str(output/'candidate-source.json')))
                .replace('RECEIPT_PATH',repr(str(prefix/RECEIPT)))
                .replace('COUNT',repr(str(build_count))))
            def fixture_plan(suite, candidate_prefix, candidate_output, *, candidate=False):
                self.assertTrue(candidate)
                return [dict(name='install', argv=[sys.executable,str(builder)],
                    inputs=(str(builder),str(output/'candidate-source.json')),
                    artifacts=(str(prefix),)),
                    dict(name='host-cli', argv=[sys.executable,'-c',
                        "from pathlib import Path; p=Path("+repr(str(host_count))+"); p.write_text(p.read_text()+'x' if p.exists() else 'x')"],
                        inputs=(str(builder),), depends=('install',))]
            identity = dict(base['source_identity'])
            args=['validate.py','--suite','release','--candidate','--prefix',str(prefix),'--output',str(output)]
            with patch.object(validate,'ROOT',root), patch.object(validate,'plan',side_effect=fixture_plan), \
                 patch.object(validate,'source_identity',side_effect=lambda: dict(identity)), patch.object(sys,'argv',args):
                validate.main(); validate.main()
                self.assertEqual(build_count.read_text(),'x')
                self.assertEqual(host_count.read_text(),'x')
                identity['git_commit']='another-commit'
                validate.main()
                self.assertEqual(build_count.read_text(),'xx')
                self.assertEqual(host_count.read_text(),'xx')
            receipt=json.loads((output/'run.json').read_text())
            self.assertTrue(receipt['candidate'])
            self.assertEqual(receipt['source'],identity)
            self.assertEqual(verify(prefix)['source_identity'],identity)


class ReleaseCommand(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root/'scripts').mkdir(); (self.root/'fake-bin').mkdir()
        self.env = dict(os.environ, TRACE=str(self.root/'.cargo-target/trace'),
            PATH=str(self.root/'fake-bin')+':'+os.environ['PATH'],
            GIT_CONFIG_COUNT='5', GIT_CONFIG_KEY_0='user.name', GIT_CONFIG_VALUE_0='Release Test',
            GIT_CONFIG_KEY_1='user.email', GIT_CONFIG_VALUE_1='test@example.invalid',
            GIT_CONFIG_KEY_2='commit.gpgsign', GIT_CONFIG_VALUE_2='false',
            GIT_CONFIG_KEY_3='tag.gpgsign', GIT_CONFIG_VALUE_3='false',
            GIT_CONFIG_KEY_4='core.excludesFile', GIT_CONFIG_VALUE_4='/dev/null')
        self.env.pop('BIRNAM_RELEASE_LOCKED', None)
        (self.root/'fake-bin/python3').symlink_to(sys.executable)
        def executable(name, text):
            p=self.root/'fake-bin'/name; p.write_text(text); p.chmod(0o755)
        real_git=shutil.which('git')
        common='import json,os,sys\nfrom pathlib import Path\np=Path(os.environ["TRACE"]);p.parent.mkdir(exist_ok=True)\n'
        executable('git', '#!'+sys.executable+'\n'+common+
            'if sys.argv[1] in ("push","tag"):\n with p.open("a") as f: f.write(json.dumps(["git",*sys.argv[1:]])+"\\n")\n'
            'if sys.argv[1]=="push": sys.exit(0)\nos.execv('+repr(real_git)+', ["git",*sys.argv[1:]])\n')
        executable('gh', '#!'+sys.executable+'\n'+common+
            'with p.open("a") as f: f.write(json.dumps(["gh",*sys.argv[1:]])+"\\n")\n')
        executable('uname', '#!/bin/sh\nif [ "$1" = -s ]; then echo Darwin; else echo arm64; fi\n')
        executable('codesign', '#!/bin/sh\nexit 0\n')
        for name in ('release.sh','release-changelog.py'):
            shutil.copy2(ROOT/'scripts'/name, self.root/'scripts'/name)
        (self.root/'VERSION').write_text('0.1.0\n')
        (self.root/'CHANGELOG.md').write_text('## [Unreleased]\n\nTest release.\n')
        shutil.copy2(ROOT/'.gitignore', self.root/'.gitignore')
        (self.root/'scripts/validate.py').write_text(common+'''
with p.open('a') as f: f.write(json.dumps(['validate',*sys.argv[1:]])+'\\n')
if os.environ.get('FAIL_QUALIFY'): sys.exit(7)
output=Path(sys.argv[sys.argv.index('--output')+1]);output.mkdir(parents=True,exist_ok=True)
(output/'run.json').write_text(json.dumps({'prefix':'/fixture/cold-candidate'}))
''')
        (self.root/'scripts/dist.sh').write_text('''#!/usr/bin/env bash
set -eu
[[ "$1" == --prefix && "$2" == /fixture/cold-candidate ]]
mkdir -p dist
echo packaged >dist/candidate.tar.gz
echo dist/candidate.tar.gz
''')
        (self.root/'scripts/release-acceptance.py').write_text(common+'''
with p.open('a') as f: f.write(json.dumps(['accept',*sys.argv[1:]])+'\\n')
output=Path(sys.argv[2]);output.mkdir(parents=True,exist_ok=True)
(output/'receipt.json').write_text(json.dumps({'result':'passed','promotable':not bool(os.environ.get('FAIL_ACCEPT'))}))
''')
        (self.root/'scripts/dist.sh').chmod(0o755)
        for args in (['init','-b','main'], ['add','.'], ['commit','-m','fixture']):
            subprocess.run([real_git,*args],cwd=self.root,env=self.env,check=True,capture_output=True)

    def release(self, *args, **env):
        return subprocess.run(['bash','scripts/release.sh',*args],cwd=self.root,
            env=dict(self.env,**env),capture_output=True,text=True,timeout=30)

    def events(self):
        p=Path(self.env['TRACE']);return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []

    def test_interrupted_candidate_resumes_without_bumping_or_publishing_early(self):
        first=self.release('patch',FAIL_QUALIFY='1')
        self.assertNotEqual(first.returncode,0,first.stdout+first.stderr)
        self.assertEqual((self.root/'VERSION').read_text().strip(),'0.1.1')
        self.assertFalse(any(e[:2] in (['git','tag'],['git','push'],['gh','release']) for e in self.events()))
        resumed=self.release('--resume')
        self.assertEqual(resumed.returncode,0,resumed.stdout+resumed.stderr)
        self.assertEqual((self.root/'VERSION').read_text().strip(),'0.1.1')
        events=self.events();qualified=[e for e in events if e[0]=='validate']
        self.assertEqual(len(qualified),2)
        self.assertEqual(qualified[0],qualified[1])
        self.assertIn('--candidate',qualified[0])
        self.assertEqual(len([e for e in events if e[0]=='accept']),1)
        self.assertIn('--qualification',next(e for e in events if e[0]=='accept'))
        self.assertLess(next(i for i,e in enumerate(events) if e[0]=='accept'),next(i for i,e in enumerate(events) if e[:2]==['git','tag']))
        self.assertEqual(subprocess.check_output(['git','rev-list','--count','HEAD'],cwd=self.root,env=self.env,text=True).strip(),'2')

    def test_invalid_changelog_stops_before_version_change_or_qualification(self):
        (self.root/'CHANGELOG.md').write_text('No release section.\n')
        subprocess.run(['git','add','CHANGELOG.md'],cwd=self.root,env=self.env,check=True,capture_output=True)
        subprocess.run(['git','commit','-m','invalid notes fixture'],cwd=self.root,env=self.env,check=True,capture_output=True)
        run=self.release('patch')
        self.assertNotEqual(run.returncode,0)
        self.assertEqual((self.root/'VERSION').read_text().strip(),'0.1.0')
        self.assertFalse(any(e[0]=='validate' for e in self.events()))
        self.assertFalse(any(e[:2] in (['git','tag'],['git','push'],['gh','release']) for e in self.events()))

    def test_nonpromotable_acceptance_never_tags_or_pushes(self):
        run=self.release('patch',FAIL_ACCEPT='1')
        self.assertNotEqual(run.returncode,0)
        self.assertFalse(any(e[:2] in (['git','tag'],['git','push'],['gh','release']) for e in self.events()))

    def test_packaging_supplied_prefix_never_builds_or_runs_matrix_and_reuses_bytes(self):
        for name in ('dist.sh','install_contract.py','release_archive.py','validation_support.py','measurement_archive.py'):
            shutil.copy2(ROOT/'scripts'/name,self.root/'scripts'/name)
        # These would fail immediately if packaging tried either old expensive path.
        for name in ('rust-install.sh','install-smoke.sh'):
            (self.root/'scripts'/name).write_text('#!/bin/sh\nexit 99\n')
            (self.root/'scripts'/name).chmod(0o755)
        prefix=self.root/'input'
        prefix.mkdir()
        (prefix/'._intentional').write_bytes(b'a real manifest file, not generated metadata\n')
        # macOS metadata must not create unmanifested AppleDouble members.
        # Exercise real tar, not an environment-variable assertion.
        self.env.pop('COPYFILE_DISABLE', None)
        if sys.platform == 'darwin':
            metadata = prefix/'metadata.txt'
            metadata.write_bytes(b'ordinary manifest bytes\n')
            subprocess.run(['xattr','-w','com.birnam.archive-test','unmanifested metadata',metadata],
                           check=True, capture_output=True)
        prefix_fixture(prefix,version='0.1.0')
        def package():
            return subprocess.run(['bash','scripts/dist.sh','--prefix',str(prefix)],cwd=self.root,
                env=self.env,capture_output=True,text=True,timeout=30)
        first=package();self.assertEqual(first.returncode,0,first.stderr)
        archive=self.root/first.stdout.strip();digest=support.sha(archive);modified=archive.stat().st_mtime_ns
        with tempfile.TemporaryDirectory() as unpacked:
            self.assertEqual(verify(acceptance.extract_candidate(archive, Path(unpacked))), verify(prefix))
        again=package();self.assertEqual(again.returncode,0,again.stderr)
        self.assertIn('reusing identical',again.stderr)
        self.assertEqual((support.sha(archive),archive.stat().st_mtime_ns),(digest,modified))
        prefix_fixture(prefix,mode='development-cache',version='0.1.0')
        self.assertNotEqual(package().returncode,0)
        self.assertEqual(support.sha(archive),digest)


if __name__ == '__main__': unittest.main()
