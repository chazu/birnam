from pathlib import Path
import subprocess,tempfile,sys
probe=Path(sys.argv[1]).resolve()
lib=Path(sys.argv[2]).resolve()
count=0
def compile(path,source):
 path.with_suffix('.c').write_text(source)
 extra=['-include',str(Path(__file__).with_name('id-protocol-stub.h'))] if path.name=='libbirnam_id.dylib' else []
 subprocess.run(['cc','-dynamiclib',*extra,str(path.with_suffix('.c')),'-o',str(path)],check=True)
def run(case,expected):
 global count
 subprocess.run([str(probe),str(case/'libbirnam_id.dylib'),expected],check=True);count+=1
def link(case,names):
 for name in names:(case/f'libbirnam_{name}.dylib').symlink_to(lib/f'libbirnam_{name}.dylib')
with tempfile.TemporaryDirectory(prefix='birnam-guard_verdict-host-') as directory:
 root=Path(directory)
 cases=[('missing','shared library missing',None),('no-provider','missing provider','int dummy;'),('null','invalid descriptor','void *birnam_guard_verdict_factory_v1(void){return 0;}')]
 values=[1,48,1,1,1,1]
 for index,name in enumerate(['version','size','create','destroy','read','write']):
  bad=values.copy();bad[index]=2 if index==0 else (47 if index==1 else 0)
  fields=','.join(map(str,bad))
  cases.append((name,'invalid descriptor',f'static unsigned long f[6]={{{fields}}};void *birnam_guard_verdict_factory_v1(void){{return f;}}'))
 cases += [('valid','ok',None)]
 for name,expected,source in cases:
  case=root/name;case.mkdir();link(case,['id','namespace','object_model','method_observation','root_registry','gc_settings','payload_layout','trace_hooks','finalizers','native_stack','native_mark','allocation_budget','collect','storage'])
  if source is not None:compile(case/'libbirnam_dispatch_cache.dylib','static unsigned long table[7]={2,56,1,1,1,1,1};void *birnam_dispatch_cache_factory_v2(void){return table;}'+source)
  elif name=='valid':link(case,['dispatch_cache'])
  run(case,expected)
 for name,body in [('missing floor dependency',''),('dependency rejected','int birnam_guard_verdict_dependency_v1(const void *p){(void)p;return 0;}')]:
  case=root/str(count);case.mkdir();link(case,['namespace','object_model','method_observation','root_registry','gc_settings','payload_layout','trace_hooks','finalizers','native_stack','native_mark','allocation_budget','collect','storage','dispatch_cache'])
  compile(case/'libbirnam_id.dylib','#include <stdlib.h>\nvoid *id_init(void){abort();}int birnam_namespace_dependency_v8(const void *p){return p!=0;}int birnam_vtable_enumeration_dependency_v2(const void *p){return p!=0;}int birnam_method_observation_dependency_v1(const void *p){return p!=0;}int birnam_root_registry_dependency_v1(const void *p){return p!=0;}int birnam_gc_settings_dependency_v1(const void *p){return p!=0;}int birnam_payload_layout_dependency_v1(const void *p){return p!=0;}int birnam_trace_hooks_dependency_v1(const void *p){return p!=0;}int birnam_finalizers_dependency_v1(const void *p){return p!=0;}int birnam_native_stack_dependency_v4(const void *p){return p!=0;}int birnam_native_mark_dependency_v1(const void *p){return p!=0;}int birnam_allocation_budget_dependency_v1(const void *p){return p!=0;}int birnam_collection_phase_dependency_v1(const void *p){return p!=0;}int birnam_storage_observation_dependency_v1(const void *p){return p!=0;}int birnam_deferred_identity_dependency_v1(const void *p){return p!=0;}int birnam_deferred_index_dependency_v1(const void *p){return p!=0;}'+'int birnam_dispatch_cache_dependency_v2(const void *p){return p!=0;}'+body)
  run(case,name)
print('All',count,'host Guard Verdict admission cases passed')
