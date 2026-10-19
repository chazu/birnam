#!/usr/bin/env python3
"""Run actual task initializers with Store/Relational unavailable on disk."""
from pathlib import Path
import shutil
import sys
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

prefix = Path(sys.argv[1]).resolve()
work = Path(tempfile.mkdtemp(prefix='birnam-task-floor-'))
floor = work / 'floor'
floor.mkdir()
native = 'id root_registry namespace boolean nil collect object_model word bytes execution portable_code string integer float prelude system_output system_input environment file_io storage vector map artifact peg tcc native_backend codegen_runtime generation'.split()
authored = 'error globalsOwner sourceRegistry cell modules lang nativeValues booleanValues integerValues blockLoopControl blockLoop range systemGc printing sourceLoader nativeLoader alienCompiler imageFormat trace globals methodInvocation stdlib stacklessControl'.split()
for name in native + authored:
    shutil.copy2(prefix / 'lib' / f'libbirnam_{name}.dylib', floor)
env = clean_environment()
env.update(BIRNAM_TEST_MODULE_DIRECTORY=str(floor), BIRNAM_GC_PRECISE='1',
           CARGO_TARGET_DIR=str(ROOT / '.cargo-target/native-message-tests'),
           CARGO_PROFILE_DEV_PANIC='unwind')
run = command(['mise', 'exec', '--', 'cargo', 'test', '--offline', '--locked',
               '-p', 'birnam-siphon', '--test', 'task_without_store', '--',
               '--include-ignored', '--nocapture'], env=env, timeout=300)
(work / 'run.log').write_text(run['output'])
receipt = {'passed': run['exit_code'] == 0, 'run': run,
           'source_sha256': {str(p.relative_to(ROOT)): sha(p) for p in [
               ROOT / 'modules/stackless-control.birnam',
               ROOT / 'rust/birnam-siphon/tests/task_without_store.rs',
               Path(__file__).resolve()]},
           'floor_sha256': {p.name: sha(p) for p in floor.iterdir()},
           'scope': 'Actual compiled task initializer, local wait/resolve/cancellation and GC lifetime. Store/Relational/Bootstrap/Image/Alien/Isolate absent. Does not prove full Host, worker or image independence.'}
save_json(work / 'results.json', receipt)
assert receipt['passed'], work
print('tasks-without-store: passed; ' + str(work / 'results.json'))
