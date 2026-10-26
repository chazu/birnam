#include "id.h"
static IdProtocol id;static Oop at,again,value_selector,frames_selector,collect_selector,run_selector,outcome_selector,fail_selector,ensure_selector;
/* [receiver, selector, argument]. Unaware native C must get an answer now. */
static Oop callback(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;
 Oop index=id_size(0),receiver=id.message(id.root,c,at,&index,1);index=id_size(1);Oop selector=id.message(id.root,c,at,&index,1);index=id_size(2);Oop arg=id.message(id.root,c,at,&index,1);
 Oop result=id.message(id.root,receiver,selector,&arg,1);
 if(!id_fault(result))return id.invalid;return id_size(71);
}
static Oop reject(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;(void)n;return id.invalid;}
static Oop get(Oop c,size_t i){Oop a=id_size(i);return id.message(id.root,c,at,&a,1);}
static Oop reenter(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;
 Oop budget=get(c,2);Oop refused=id.message(id.root,get(c,0),again,&budget,1);if(!id_fault(refused))return id.invalid;
 Oop collected=id.message(id.root,get(c,3),collect_selector,0,0);if(id_fault(collected))return collected;
 return id.message(id.root,get(c,1),value_selector,0,0);}
static Oop inspect_frames(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;
 return id.message(id.root,c,frames_selector,0,0);}
static Oop run_other(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;if(n)return id.invalid;
 Oop block=get(c,1);return id.message(id.root,get(c,0),run_selector,&block,1);}
/* The same unaware callbacks used by the retained task-control smoke test.
   Context owns [cleanup Block, failure reason]. */
static Oop observe(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=1)return id.invalid;return id.message(id.root,a[0],outcome_selector,0,0);}
static Oop fail_task(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=1)return id.invalid;Oop reason=get(c,1);return id.message(id.root,a[0],fail_selector,&reason,1);}
static Oop ensure_task(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;if(n!=2)return id.invalid;Oop args[]={a[1],get(c,0)};return id.message(id.root,a[0],ensure_selector,args,2);}
static const IdLiteral literals[]={LIT("at:"),LIT("callback"),LIT("rejectLookup"),LIT("reenter"),LIT("continueWithBudget:"),LIT("value"),LIT("inspectFrames"),LIT("liveFrameCount"),LIT("collectPrecisely"),LIT("runOther"),LIT("runBlock:"),LIT("observe:"),LIT("failTask:"),LIT("ensureTask:body:"),LIT("outcome"),LIT("fail:"),LIT("ensure:cleanup:")};
static const IdMethodSpec methods[]={{1,callback},{2,reject},{3,reenter},{6,inspect_frames},{9,run_other},{11,observe},{12,fail_task},{13,ensure_task}};
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t n,const Oop*a,size_t count){(void)a;if(n!=17||count)return p->invalid;id=*p;at=s[0];again=s[4];value_selector=s[5];frames_selector=s[7];collect_selector=s[8];run_selector=s[10];outcome_selector=s[14];fail_selector=s[15];ensure_selector=s[16];return image;}
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,17,methods,8,initialize};
