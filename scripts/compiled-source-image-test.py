#!/usr/bin/env python3
"""Owned compilation snapshots and original source identities across source-free images."""
import json,sys,tempfile
from pathlib import Path
from validation_support import ROOT,clean_environment,command,save_json,sha
prefix=Path(sys.argv[1]).resolve();work=Path(tempfile.mkdtemp(prefix='birnam-compiled-source-'))
env=clean_environment();env['BIRNAM_GC_PRECISE']='1'
writer=work/'save.birnam';image=work/'compilation.image'
fixture=ROOT/'tests/fixtures/compiled-source-image.birnam'
writer.write_text(fixture.read_text().replace('IMAGE_PATH',json.dumps(str(image))))
receipt={'passed':False,'prefix':str(prefix),'runs':[],'fixture_sha256':sha(fixture)}
for name,args,expected in [('save',[writer],'#saved\n'),('mapped',[image],'compiled-source-image: passed\n'),('rebuilt',['--rebuild',image],'compiled-source-image: passed\n')]:
    result=command([prefix/'bin/birnam',*args],cwd=work,env=env,timeout=300)
    receipt['runs'].append(dict(name=name,**result));save_json(work/'results.json',receipt)
    (work/(name+'.log')).write_text(result['output'])
    assert result['exit_code']==0 and not result['stderr'] and result['stdout']==expected,(work,name,result)
    if name=='save':receipt['image_sha256']=sha(image);writer.unlink()
receipt['passed']=True;save_json(work/'results.json',receipt)
print('compiled-source images: passed; '+str(work/'results.json'))
