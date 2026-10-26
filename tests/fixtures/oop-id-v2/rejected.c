#include "id.h"
#include <stdlib.h>
static Oop never(const IdProtocol*p,Oop m,const Oop*l,size_t n,const Oop*a,size_t c){(void)p;(void)m;(void)l;(void)n;(void)a;(void)c;abort();}
const IdModule birnam_native_module_v2={1,sizeof(IdModule),0,0,0,0,never};
