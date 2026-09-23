/* Independent executable probes for Execution's canonical descriptor seam.
 * Fixture contexts have test-process lifetime. The declared language owner is
 * the only owner of probe data; every access goes through current Id lookup. */
#include "id_protocol.h"
#include <stdlib.h>
#include <string.h>

typedef struct { BirnamProtocol id; BirnamObject ns; BirnamObject owner; uintptr_t mode; uintptr_t tokens[128]; size_t retained; } Probe;
static BirnamOutcome send(Probe *p, BirnamObject r, const char *name, const BirnamOperand *a, size_t n) {
    BirnamOperand text={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)name,strlen(name)}};
    BirnamOutcome selector=p->id.message(p->id.context,p->id.receiver,p->id.intern,&text,1);
    if(selector.status!=BIRNAM_LOADED)return selector;
    return p->id.message(p->id.context,r,selector.value.data.object,a,n);
}
static BirnamObject named(Probe *p,const char *name) {
    BirnamOperand text={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)name,strlen(name)}};
    BirnamOutcome symbol=p->id.message(p->id.context,p->id.receiver,p->id.intern,&text,1);
    return send(p,p->ns,"at:",&symbol.value,1).value.data.object;
}
static BirnamOperand integer(Probe *p,uintptr_t word){
    BirnamOperand value={.kind=BIRNAM_WORD,.data.word=word};
    BirnamOperand answer=send(p,named(p,"Integer"),"fromWord:",&value,1).value;
    if(p->retained==128)abort();
    p->tokens[p->retained++]=send(p,named(p,"Roots"),"retain:",&answer,1).value.data.word;
    return answer;
}
static BirnamOperand slot(Probe *p,BirnamObject m,uintptr_t i){
    BirnamOperand index=integer(p,i);return send(p,m,"oopAt:",&index,1).value;
}
static BirnamOutcome step_body(void *context,BirnamObject machine,const BirnamOperand *args,size_t count){
    Probe *p=context;
    uintptr_t pc=count?send(p,named(p,"Integer"),"wordOf:",args,1).value.data.word:0;
    if(p->mode==1 || p->mode==6){
        BirnamOperand i=integer(p,1);return send(p,machine,"returnAt:",&i,1);
    }
    if(p->mode==5){
        if(pc!=3){
            BirnamOperand i=integer(p,pc==0?1:3);return send(p,machine,"continueAt:",&i,1);
        }
        BirnamOperand i=integer(p,1);return send(p,machine,"returnAt:",&i,1);
    }
    if(pc==1){BirnamOperand i=integer(p,5);return send(p,machine,"returnAt:",&i,1);}
    if(p->mode==2 || p->mode==3){
        BirnamOperand a[]={integer(p,5),integer(p,1)};
        return send(p,machine,p->mode==2?"suspendAt:continueAt:":"captureAt:continueAt:",a,2);
    }
    /* owner Vector = [target, selector, argument]; it lives in frame slot 1. */
    BirnamObject owner=slot(p,machine,1).data.object;
    for(uintptr_t i=0;i<3;i++){
        BirnamOperand ix=integer(p,i),value=send(p,owner,"at:",&ix,1).value;
        BirnamOperand a[]={integer(p,i+2),value};
        BirnamOutcome written=send(p,machine,"oopAt:put:",a,2);
        if(written.status!=BIRNAM_LOADED)return written;
    }
    BirnamOperand i=integer(p,0);
    return send(p,machine,p->mode==7?"tailCallSite:":"callSite:",&i,1);
}
static BirnamOutcome step(void *context,BirnamObject machine,const BirnamOperand *args,size_t count){
    Probe local=*(Probe *)context;local.retained=0;
    BirnamOutcome result=step_body(&local,machine,args,count);
    while(local.retained){BirnamOperand token={.kind=BIRNAM_WORD,.data.word=local.tokens[--local.retained]};send(&local,named(&local,"Roots"),"release:",&token,1);}
    return result;
}
static BirnamOutcome inline_entry(void *context,BirnamObject machine,const BirnamOperand *args,size_t count){
    (void)args;(void)count;
    Probe local=*(Probe *)context;local.retained=0;Probe *p=&local;
    BirnamOutcome result={.status=BIRNAM_LOADED,.value={.kind=BIRNAM_UNIT}};
    if(p->mode!=1){
        BirnamOperand index=integer(p,0);
        result=send(p,p->owner,"at:",&index,1);
        if(result.status==BIRNAM_LOADED && p->mode>=2){
            p->tokens[p->retained++]=send(p,named(p,"Roots"),"retain:",&result.value,1).value.data.word;
            BirnamOperand a[]={integer(p,0),result.value};
            result=send(p,machine,p->mode==3?"recoverAt:withGraph:":"recoverAt:with:",a,2);
            if(result.status==BIRNAM_LOADED)result.value=(BirnamOperand){.kind=BIRNAM_UNIT};
        }
    }
    while(local.retained){BirnamOperand token={.kind=BIRNAM_WORD,.data.word=local.tokens[--local.retained]};send(&local,named(&local,"Roots"),"release:",&token,1);}
    return result;
}
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count){
    if(id && id->version==1 && count==5 && args && args[0].kind==BIRNAM_OBJECT && args[1].kind==BIRNAM_OBJECT && args[2].kind==BIRNAM_OBJECT && args[3].kind==BIRNAM_WORD && args[4].kind==BIRNAM_WORD){
        Probe *p=calloc(1,sizeof(*p));if(!p)return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST};
        p->id=*id;p->ns=args[0].data.object;p->owner=args[2].data.object;p->mode=args[3].data.word;
        BirnamOperand a[]={ {.kind=BIRNAM_NATIVE_ENTRY,.data.entry={inline_entry,p}},integer(p,args[4].data.word),args[2] };
        BirnamOutcome result=send(p,args[1].data.object,"withInline:at:owner:",a,3);
        while(p->retained){BirnamOperand token={.kind=BIRNAM_WORD,.data.word=p->tokens[--p->retained]};send(p,named(p,"Roots"),"release:",&token,1);}
        if(result.status!=BIRNAM_LOADED)free(p);
        return result;
    }
    if(!id || id->version!=1 || count!=5 || !args || args[0].kind!=BIRNAM_OBJECT || args[1].kind!=BIRNAM_BYTES || args[2].kind!=BIRNAM_BYTES || args[3].kind!=BIRNAM_OBJECT || args[4].kind!=BIRNAM_WORD)
        return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST};
    Probe *p=malloc(sizeof(*p));if(!p)return (BirnamOutcome){.status=BIRNAM_INVALID_REQUEST};
    *p=(Probe){.id=*id,.ns=args[0].data.object,.mode=args[4].data.word};
    BirnamOperand a[]={ {.kind=BIRNAM_NATIVE_ENTRY,.data.entry={step,p}},args[1],args[2],args[3] };
    BirnamOutcome result=send(p,named(p,"Method"),"fromStep:descriptor:selection:owner:",a,4);
    if(result.status!=BIRNAM_LOADED)free(p);
    return result;
}
__attribute__((visibility("default")))
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
