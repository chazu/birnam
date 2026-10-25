#include "../oop-id-v2/id.h"
static Oop initialize(const IdProtocol*p,Oop image,const Oop*l,size_t n,const Oop*a,size_t count){(void)image;(void)l;(void)n;return count==1?a[0]:p->invalid;}
const IdModule birnam_native_module_v2={2,sizeof(IdModule),0,0,0,0,initialize};
