/* Independent generated module; the bound context is ordinary Oop data. */
typedef unsigned long Size;
typedef unsigned int U32;
typedef void *Oop;
typedef Oop (*Entry)(Oop,Oop,Oop,const Oop*,Size);
typedef Oop (*Send)(Oop,Oop,Oop,const Oop*,Size);
typedef struct { U32 version,bytes; Send message; Oop root,intern,load,at,seeds,invalid; } Id;
typedef struct { Id common; Oop (*control)(Oop,Oop,const Oop*,Size); } Protocol;
typedef struct { const unsigned char *bytes; Size length; } Literal;
typedef struct { Size name; Entry entry; U32 convention; } Method;
typedef Oop (*Initialize)(const Protocol*,Oop,const Oop*,Size,const Oop*,Size);
typedef struct { U32 version,bytes; const Literal*literals; Size literal_count; const Method*methods; Size method_count; Initialize initialize; } Module;
static Protocol api;
static Oop symbols[10], gc;
static Oop context(Oop root,Oop c,Oop r,const Oop*a,Size n) {
 (void)root;(void)r;(void)a;return n?api.common.invalid:c;
}
static Oop triple(Oop root,Oop c,Oop r,const Oop*a,Size n) {
 (void)root;if(n!=1)return api.common.invalid;
 Oop result=api.common.message(api.common.root,gc,symbols[5],0,0);
 if(((Size)result&3)==2)return result;
 Oop values[]={c,r,a[0]};return api.common.message(api.common.root,api.common.root,symbols[3],values,3);
}
static Oop continuing(Oop root,Oop c,Oop r,const Oop*a,Size n) {
 (void)root;(void)r;if(n!=1)return api.common.invalid;
 Oop values[]={a[0],c};return api.control(api.common.root,symbols[4],values,2);
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,Size n) {
 (void)root;(void)r;if(n!=3)return api.common.invalid;
 Oop index=(Oop)1;
 Oop expected=api.common.message(api.common.root,c,symbols[9],&index,1);
 if(expected==a[2]){Oop args[]={(Oop)3,(Oop)3};return api.common.message(api.common.root,c,symbols[8],args,2);}
 return (Oop)0;
}
static Oop initialize(const Protocol*p,Oop unit,const Oop*s,Size n,const Oop*a,Size count) {
 if(n!=10||count!=1)return p->common.invalid;
 api=*p;for(Size i=0;i<n;i++)symbols[i]=s[i];
 gc=api.common.message(api.common.root,a[0],api.common.at,&symbols[6],1);
 if(((Size)gc&3)==2)return gc;
 return unit;
}
#define L(s) {(const unsigned char*)(s),sizeof(s)-1}
static const Literal literals[]={L("context"),L("triple:"),L("continuing"),L("sequence"),L("answerTo:with:"),L("collectPrecisely"),L("GC"),L("allocation:bytes:identity:"),L("at:put:"),L("at:")};
static const Method methods[]={{0,context,0},{1,triple,0},{2,continuing,1},{7,allocation,0}};
const Module birnam_native_module_v3={3,sizeof(Module),literals,10,methods,4,initialize};
