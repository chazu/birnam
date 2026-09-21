from pathlib import Path
import subprocess,tempfile,sys
probe=Path(sys.argv[1]).resolve()
lib=Path(sys.argv[2]).resolve()
count=0
def compile(path,source):
 path.with_suffix('.c').write_text(source)
 subprocess.run(['cc','-dynamiclib',str(path.with_suffix('.c')),'-o',str(path)],check=True)
def run(case,expected):
 global count
 subprocess.run([str(probe),str(case/'libbirnam_id.dylib'),expected],check=True);count+=1
def link(case,names):
 for name in names:(case/f'libbirnam_{name}.dylib').symlink_to(lib/f'libbirnam_{name}.dylib')
with tempfile.TemporaryDirectory(prefix='birnam-allocation_budget-host-') as directory:
 root=Path(directory)
 cases=[('missing','open failed',None),('no-provider','missing provider','int dummy;'),('null','invalid descriptor','void *birnam_allocation_budget_factory_v1(void){return 0;}')]
 values=[1,64,1,1,1,1,1,1]
 for index,name in enumerate(['version','size','create','destroy','read','replace','charge','preview']):
  bad=values.copy();bad[index]=2 if index==0 else (63 if index==1 else 0)
  fields=','.join(map(str,bad))
  cases.append((name,'invalid descriptor',f'static unsigned long f[8]={{{fields}}};void *birnam_allocation_budget_factory_v1(void){{return f;}}'))
 cases += [('valid','ok',None)]
 for name,expected,source in cases:
  case=root/name;case.mkdir();link(case,['id','dispatch_cache','namespace','object_model','method_observation','root_registry','gc_settings','payload_layout','trace_hooks','finalizers','native_stack','native_mark','collect','storage'])
  if source is not None:compile(case/'libbirnam_allocation_budget.dylib',source)
  elif name=='valid':link(case,['allocation_budget'])
  run(case,expected)
 for name,body in [('missing floor dependency',''),('dependency rejected','int birnam_allocation_budget_dependency_v1(const void *p){(void)p;return 0;}')]:
  case=root/str(count);case.mkdir();link(case,['namespace','object_model','method_observation','root_registry','gc_settings','payload_layout','trace_hooks','finalizers','native_stack','native_mark','allocation_budget'])
  compile(case/'libbirnam_id.dylib','#include <stdlib.h>\nvoid *id_init(void){abort();}int birnam_namespace_dependency_v8(const void *p){return p!=0;}int birnam_vtable_enumeration_dependency_v2(const void *p){return p!=0;}int birnam_method_observation_dependency_v1(const void *p){return p!=0;}int birnam_root_registry_dependency_v1(const void *p){return p!=0;}int birnam_gc_settings_dependency_v1(const void *p){return p!=0;}int birnam_payload_layout_dependency_v1(const void *p){return p!=0;}int birnam_trace_hooks_dependency_v1(const void *p){return p!=0;}int birnam_finalizers_dependency_v1(const void *p){return p!=0;}int birnam_native_stack_dependency_v4(const void *p){return p!=0;}int birnam_native_mark_dependency_v1(const void *p){return p!=0;}'+body)
  run(case,name)
print('All',count,'host Allocation Budget admission cases passed')
