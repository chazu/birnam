#!/usr/bin/env python3
"""Foreign wrapper recovery through mapped/rebuilt images and baked entry."""
import json, subprocess, tempfile, shutil, sys
from pathlib import Path
binary=Path(sys.argv[1]).resolve()/'bin/birnam'
with tempfile.TemporaryDirectory(prefix='birnam-alien-image-') as temporary:
    work=Path(temporary); source=work/'save.birnam'; image=work/'foreign.image'; baked=work/'bundle/bin/foreign'
    baked.parent.mkdir(parents=True)
    shutil.copytree(binary.parent.parent/'lib',work/'bundle/lib')
    source.write_text('[Alien wrap: "labs" signature: "l l" as: #foreignAbsolute on: Integer]; [Globals at: #main put: [| [System println: [[0 - 7] foreignAbsolute]]]]; [System saveImage: '+json.dumps(str(image))+']')
    def run(args):
        result=subprocess.run(args,cwd=work,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=90)
        assert result.returncode==0,(args,result.returncode,result.stdout,result.stderr)
        assert not result.stderr,(args,result.stderr)
        return result.stdout
    assert run([binary,source])==b''
    source.unlink()
    assert run([binary,image])==b'7\n'
    assert run([binary,'--rebuild',image])==b'7\n'
    assert run([binary,'--bake',image,baked])==b''
    assert run([baked])==b'7\n'
    print('Alien/Image: saved wrapper object survives source deletion, mapped/rebuilt recovery and baked entry')
