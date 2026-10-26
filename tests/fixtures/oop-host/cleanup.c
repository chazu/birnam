#include "../oop-id-v2/id.h"
/* Force collection at Host cleanup without using source-code cleanup to
   implement release: itself. Context is [original Method, Roots, GC]. */
static IdProtocol id;
static Oop names[10];
static Oop field(Oop c,size_t i){Oop index=id_size(i);return id.message(id.root,c,id.at,&index,1);}
static Oop release_collecting(Oop root,Oop c,Oop r,const Oop*a,size_t n){
 (void)root;(void)r;if(n!=1)return id.invalid;
 Oop original=field(c,0),roots=field(c,1),gc=field(c,2);
 if(id_fault(original)||id_fault(roots)||id_fault(gc))return id.invalid;
 Oop result=id.message(id.root,gc,names[3],0,0);if(id_fault(result))return result;
 Oop arguments=id.message(id.root,id.root,names[4],a,n);if(id_fault(arguments))return arguments;
 Oop values[]={roots,arguments};return id.message(id.root,original,names[5],values,2);
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*l,size_t n,const Oop*a,size_t count){
 id=*p;if(n!=10||count!=1)return id.invalid;
 for(size_t i=0;i<n;i++)names[i]=l[i];
 Oop method=id.message(id.root,image,names[8],&names[0],1);if(id_fault(method))return method;
 Oop roots=id.message(id.root,a[0],id.at,&names[1],1);
 Oop gc=id.message(id.root,a[0],id.at,&names[2],1);
 Oop original=id.message(id.root,roots,names[6],&names[7],1);
 if(id_fault(roots)||id_fault(gc)||id_fault(original))return id.invalid;
 Oop values[]={original,roots,gc};
 Oop context=id.message(id.root,id.root,names[4],values,3);if(id_fault(context))return context;
 return id.message(id.root,method,names[9],&context,1);
}

static const IdLiteral literals[]={LIT("collectingRelease"),LIT("Roots"),LIT("GC"),LIT("collectPrecisely"),LIT("sequence"),LIT("invokeOn:arguments:"),LIT("methodFor:"),LIT("release:"),LIT("methodAt:"),LIT("withContext:")};
static const IdMethodSpec methods[]={{0,release_collecting}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,10,methods,1,initialize};
