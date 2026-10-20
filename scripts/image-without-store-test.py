#!/usr/bin/env python3
"""Actual Image/compiled-owner save/load with Store providers unavailable on disk."""
from pathlib import Path
import shutil
import sys
import tempfile
from validation_support import ROOT, clean_environment, command, save_json, sha
prefix=Path(sys.argv[1]).resolve()
work=Path(tempfile.mkdtemp(prefix='birnam-image-floor-'));floor=work/'floor';floor.mkdir()
native='id root_registry namespace boolean nil collect object_model word bytes execution portable_code string integer float prelude system_output system_input environment file_io storage vector map artifact peg tcc native_backend codegen_runtime generation alien image'.split()
authored='error globalsOwner sourceRegistry cell modules lang nativeValues booleanValues integerValues blockLoopControl blockLoop range systemGc printing sourceLoader nativeLoader alienCompiler alienRecipes imageFormat trace globals methodInvocation stdlib'.split()
for name in native+authored:shutil.copy2(prefix/'lib'/f'libbirnam_{name}.dylib',floor/f'libbirnam_{name}.dylib')
env=clean_environment();env.update(BIRNAM_TEST_MODULE_DIRECTORY=str(floor),BIRNAM_GC_PRECISE='1',CARGO_TARGET_DIR=str(ROOT/'.cargo-target/native-message-tests'),CARGO_PROFILE_DEV_PANIC='unwind')
receipt=dict(passed=False,scope='Actual native Image and compiled initializers on an assembled floor, not full Host/cold-build/worker acceptance.',runs=[],source_sha256={str(p.relative_to(ROOT)):sha(p) for p in [Path(__file__).resolve(),ROOT/'rust/birnam-siphon/tests/image_without_store.rs',ROOT/'rust/birnam-image/src/native.rs',ROOT/'modules/image-format.birnam',ROOT/'modules/stdlib.birnam',ROOT/'tests/fixtures/documentation-hooks.birnam']})
def run(name,args,environment):
 r=command(args,env=environment,timeout=300);(work/(name+'.log')).write_text(r['output']);receipt['runs'].append(dict(name=name,**r));save_json(work/'results.json',receipt);assert r['exit_code']==0,(name,work,r['output'][-2000:])
# Produce the optional image on the same floor plus Store alone. This rules
# out rejection caused by another service from the full Host composition.
optional=work/'optional.image';optional_floor=work/'optional-floor'
shutil.copytree(floor,optional_floor)
shutil.copy2(prefix/'lib/libbirnam_store.dylib',optional_floor/'libbirnam_store.dylib')
for mode in ['optional-save','optional-mapped','optional-rebuild']:
 e=dict(env,BIRNAM_TEST_MODULE_DIRECTORY=str(optional_floor),BIRNAM_IMAGE_FLOOR_MODE=mode,BIRNAM_IMAGE_FLOOR_PATH=str(optional))
 run(mode,['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','image_without_store','--','--include-ignored','--nocapture'],e)
assert optional.exists()
image=work/'core.image';resaved=work/'resaved.image'
for mode,path in [('save',image),('mapped',image),('rebuild',image),('resave',image),('mapped',resaved),('rebuild',resaved),('missing',optional)]:
 e=dict(env,BIRNAM_IMAGE_FLOOR_MODE=mode,BIRNAM_IMAGE_FLOOR_PATH=str(path),BIRNAM_IMAGE_FLOOR_OUTPUT=str(resaved))
 run(mode+'-'+path.stem,['mise','exec','--','cargo','test','--offline','--locked','-p','birnam-siphon','--test','image_without_store','--','--include-ignored','--nocapture'],e)
receipt.update(passed=True,image_sha256=sha(image),resaved_sha256=sha(resaved),floor_sha256={p.name:sha(p) for p in floor.iterdir()});save_json(work/'results.json',receipt);print('image-without-store: passed; '+str(work/'results.json'))
