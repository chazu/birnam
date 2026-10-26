#include "id.h"
#include <stdlib.h>
static Oop initialize(const IdContinuationProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){(void)p;(void)image;(void)s;(void)n;(void)a;(void)count;abort();}
#ifdef BAD_CONVENTION
static Oop bad(Oop id,Oop c,Oop r,const Oop*a,size_t n){(void)id;(void)c;(void)r;(void)a;(void)n;abort();}
static const IdLiteral literals[]={LIT("bad")};
static const IdContinuationMethodSpec methods[]={{0,bad,77}};
const IdContinuationModule birnam_native_module_v3={3,sizeof(IdContinuationModule),literals,1,methods,1,initialize};
#else
/* A v3 export cannot accept a v2 version even though the outer layout matches. */
const IdContinuationModule birnam_native_module_v3={2,sizeof(IdContinuationModule),0,0,0,0,initialize};
#endif
