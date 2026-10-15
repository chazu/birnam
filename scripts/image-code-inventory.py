#!/usr/bin/env python3
"""Inspect exact duplicate object payloads in an existing BIRNAMO1 image bundle.

Read-only diagnostic, not image admission. Uses mmap and copied digests; does not
load native code. Hash matches are confirmed byte-for-byte before counting reuse.
"""
import argparse
import hashlib
import json
import mmap
from pathlib import Path
import struct

p=argparse.ArgumentParser(description=__doc__)
p.add_argument('images',nargs='+',type=Path)
a=p.parse_args()
results=[]
for path in a.images:
    with path.open('rb') as f, mmap.mmap(f.fileno(),0,access=mmap.ACCESS_READ) as data:
        start=data.find(b'BIRNAMO1')
        assert start>=0, 'No BIRNAMO1 bundle'
        count=struct.unpack_from('<I',data,start+40)[0]
        assert 0<count<1000000
        inventory=data.find(b'birnam.portable-inventory.v1\0')
        assert 0 <= inventory < start
        programs=struct.unpack_from('<I',data,inventory+72)[0]
        assert 2*programs <= count
        cursor=start+44
        groups={}
        sizes=[]
        view=memoryview(data)
        for i in range(count):
            length=struct.unpack_from('<Q',data,cursor)[0];cursor+=8
            assert length>0 and cursor+length<=len(data)
            assert data[cursor:cursor+4]==b'\x7fELF'
            digest=hashlib.sha256(view[cursor:cursor+length]).hexdigest()
            if digest in groups:
                group=groups[digest]
                assert length==group['bytes'] and view[cursor:cursor+length]==view[group['offset']:group['offset']+length]
                group['count']+=1
            else: groups[digest]=dict(offset=cursor,bytes=length,count=1)
            sizes.append(length);cursor+=length
        del view
        duplicate_bytes=sum(g['bytes']*(g['count']-1) for g in groups.values())
        results.append(dict(path=str(path.resolve()),image_bytes=len(data),bundle_offset=start,bundle_bytes=cursor-start,
                            objects=count,unique_objects=len(groups),object_bytes=sum(sizes),duplicate_object_bytes=duplicate_bytes,
                            programs=programs,resumable_body_bytes=sum(sizes[:2*programs:2]),synchronous_body_bytes=sum(sizes[1:2*programs:2]),
                            named_adapter_objects=count-2*programs,named_adapter_bytes=sum(sizes[2*programs:]),
                            largest_duplicate_groups=sorted([g for g in groups.values() if g['count']>1],key=lambda g:g['bytes']*(g['count']-1),reverse=True)[:10]))
print(json.dumps(results,indent=2))
