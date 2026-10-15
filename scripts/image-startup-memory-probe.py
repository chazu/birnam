#!/usr/bin/env python3
"""On macOS, sample an owned host process through startup and at a blocked input boundary.

Diagnostic, not a timing benchmark. ps samples can miss the instantaneous peak.
The process runs unchanged production code, optionally collecting before blocking.
"""
import argparse,json,os,select,subprocess,tempfile,time
from pathlib import Path
from validation_support import clean_environment,save_json
p=argparse.ArgumentParser(description=__doc__);p.add_argument('prefix',type=Path);a=p.parse_args()
work=Path(tempfile.mkdtemp(prefix='birnam-startup-memory-'));results=[]
for gc in [False,True]:
    source=work/('gc.birnam' if gc else 'plain.birnam')
    source.write_text(('[System gc];\n' if gc else '')+'[System println: "MEMORY_READY"]; [System readLine]\n')
    proc=subprocess.Popen([a.prefix.resolve()/'bin/birnam',source],env=clean_environment(),stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    samples=[];output=b'';started=time.monotonic();ready=False
    try:
        while time.monotonic()-started<90:
            if proc.poll() is not None: break
            rss=subprocess.run(['ps','-o','rss=','-p',str(proc.pid)],capture_output=True,text=True).stdout.strip()
            if rss:samples.append([time.monotonic()-started,int(rss)*1024])
            if select.select([proc.stdout],[],[],.05)[0]:
                output+=os.read(proc.stdout.fileno(),4096)
                if b'MEMORY_READY\n' in output:ready=True;break
        assert ready,(proc.poll(),output)
        rss=int(subprocess.check_output(['ps','-o','rss=','-p',str(proc.pid)],text=True).strip())*1024
        vm=subprocess.run(['vmmap','-summary',str(proc.pid)],capture_output=True,text=True,timeout=30)
        (work/('gc-vmmap.txt' if gc else 'plain-vmmap.txt')).write_text(vm.stdout+vm.stderr)
        out,err=proc.communicate(b'\n',timeout=30);output+=out
        assert proc.returncode==0 and not err,(proc.returncode,output,err)
        results.append(dict(gc=gc,prefix=str(a.prefix.resolve()),sampled_peak_rss=max(s[1] for s in samples),ready_rss=rss,samples=samples,stdout=output.decode(),vmmap_exit=vm.returncode))
    finally:
        if proc.poll() is None:proc.kill();proc.wait()
save_json(work/'results.json',dict(scope='Sampled RSS in bytes; ps observer overhead; explicit GC runs after startup.',runs=results))
print(json.dumps([{k:r[k] for k in ['gc','sampled_peak_rss','ready_rss','vmmap_exit']} for r in results],indent=2));print(work/'results.json')
