#!/usr/bin/env python3
"""Load the actual Globals initializer without Store, then test explicit opt-in."""
import json
from pathlib import Path
import shutil
import sys
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha

prefix=Path(sys.argv[1]).resolve()
work=Path(tempfile.mkdtemp(prefix='birnam-globals-floor-'));floor=work/'floor';floor.mkdir()
optional=work/'optional';optional.mkdir()
for name in 'id root_registry namespace boolean nil collect object_model word bytes execution portable_code string integer float prelude file_io vector map artifact tcc native_backend codegen_runtime error globalsOwner'.split():
 shutil.copy2(prefix/'lib'/f'libbirnam_{name}.dylib',floor)
for name in ['store','relational']:
 shutil.copy2(prefix/'lib'/f'libbirnam_{name}.dylib',optional)
env=clean_environment();env.update(BIRNAM_TEST_MODULE_DIRECTORY=str(floor),BIRNAM_OPTIONAL_MODULE_DIRECTORY=str(optional),CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),CARGO_PROFILE_DEV_PANIC='unwind')
run=command(['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','globals_without_store','--','--include-ignored'],env=env,timeout=240)
(work/'run.log').write_text(run['output'])
receipt={'passed':run['exit_code']==0,'run':run,'source_sha256':{str(p.relative_to(ROOT)):sha(p) for p in [ROOT/'modules/globals-owner.birnam',ROOT/'modules/error.birnam',ROOT/'rust/birnam-siphon/tests/globals_without_store.rs',Path(__file__).resolve()]},'floor_sha256':{p.name:sha(p) for p in floor.iterdir()},'optional_sha256':{p.name:sha(p) for p in optional.iterdir()},'scope':'Actual compiled initializer and core dependencies, no Globals seed, Store/Relational/Bootstrap/Generation/Lang unavailable in the floor. Optional libraries are loaded explicitly only after independent binding checks. This does not prove the complete Store-free Host or Image path.'}
save_json(work/'results.json',receipt);assert receipt['passed'],work
print('globals-without-store: passed; '+str(work/'results.json'))
