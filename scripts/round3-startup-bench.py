#!/usr/bin/env python3
"""Interleave fresh-process startup through installed images; warm files, no source reload."""
import argparse, hashlib, json, os, platform, statistics, subprocess, tempfile, time
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('before');p.add_argument('after');p.add_argument('--runs',type=int,default=5)
a=p.parse_args();env={k:v for k,v in os.environ.items() if not k.startswith('BIRNAM_')}
results={label:[] for label in ['before','after']}
with tempfile.TemporaryDirectory(prefix='birnam-round3-startup-') as d:
 source=Path(d)/'ready.birnam';source.write_text('[System println: "ROUND3_READY"];\n')
 for sample in range(a.runs+1):
  for label in (['before','after'] if sample%2==0 else ['after','before']):
   start=time.perf_counter();r=subprocess.run([getattr(a,label),str(source)],env=env,cwd=d,capture_output=True,text=True,check=True);elapsed=time.perf_counter()-start
   assert r.stdout.strip()=='ROUND3_READY',(label,r.stdout,r.stderr)
   if sample:results[label].append(elapsed)
metadata={}
for label in results:
 binary=Path(getattr(a,label));receipt=json.loads((binary.parent.parent/'share/birnam/rust-install.json').read_text())
 metadata[label]={'binary':str(binary),'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'source_manifest_sha256':receipt['source_manifest_sha256'],'source_identity':receipt['source_identity'],'seconds':results[label],'median_seconds':statistics.median(results[label])}
print(json.dumps({'scope':'Fresh process loads installed startup image, compiles and executes one println; includes process startup, warm filesystem caches; alternating order, one excluded warmup per binary.','machine':platform.platform(),'results':metadata,'speedup':metadata['before']['median_seconds']/metadata['after']['median_seconds']},indent=2))
