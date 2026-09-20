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
with tempfile.TemporaryDirectory(prefix='birnam-key_index-host-') as directory:
 root=Path(directory)
 cases=[('missing','shared library missing',None),('no-provider','missing provider','int dummy;'),('null','invalid descriptor','void *birnam_key_index_factory_v1(void){return 0;}')]
 values=[1,56,1,1,1,1,1]
 for index,name in enumerate(['version','size','destroy','is_empty','get','insert','clear']):
  bad=values.copy();bad[index]=2 if index==0 else (55 if index==1 else 0)
  fields=','.join(map(str,bad))
  cases.append((name,'invalid descriptor',f'static unsigned long f[7]={{{fields}}};void *birnam_key_index_factory_v1(void){{return f;}}'))
 cases += [('valid','ok',None)]
 for name,expected,source in cases:
  case=root/name;case.mkdir();link(case,['id','namespace','dispatch_cache','method_observation','root_registry','gc_settings','payload_layout','trace_hooks','finalizers','native_stack','native_mark','allocation_budget','collect','storage'])
  if source is not None:compile(case/'libbirnam_object_model.dylib','static unsigned long table[4]={2,32,1,1};void *birnam_vtable_enumeration_factory_v2(void){return table;}static unsigned long epoch[6]={1,48,1,1,1,1};void *birnam_mutation_epoch_factory_v1(void){return epoch;}'+source)
  elif name=='valid':link(case,['object_model'])
  run(case,expected)
 for name,body in [('missing floor dependency',''),('dependency rejected','int birnam_key_index_dependency_v1(const void *p){(void)p;return 0;}')]:
  case=root/str(count);case.mkdir();link(case,['namespace','object_model','method_observation','root_registry','gc_settings','payload_layout','trace_hooks','finalizers','native_stack','native_mark','allocation_budget','collect','storage','dispatch_cache'])
  compile(case/'libbirnam_id.dylib','#include <stdlib.h>\nvoid *id_init(void){abort();}int birnam_namespace_dependency_v8(const void *p){return p!=0;}int birnam_vtable_enumeration_dependency_v2(const void *p){return p!=0;}int birnam_mutation_epoch_dependency_v1(const void *p){return p!=0;}'+body)
  run(case,name)
print('All',count,'host Key Index admission cases passed')
