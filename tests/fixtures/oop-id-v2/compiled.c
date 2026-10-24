/* Appended to the independent ABI header by the acceptance driver. */
static IdProtocol api;
static Oop namespace,unit,keys[9];
static Oop echo(Oop root,Oop context,Oop receiver,const Oop *args,size_t count){
 if(context!=unit||count!=1)return api.invalid;
 Oop gc=api.message(root,namespace,keys[2],&keys[3],1);
 if(id_fault(gc))return gc;
 Oop collected=api.message(root,gc,keys[4],0,0);
 return id_fault(collected)?collected:args[0];
}
static Oop ready(Oop root,Oop context,Oop receiver,const Oop *args,size_t count){return id_size(1);}
static Oop checkpoint(Oop root,Oop context,Oop receiver,const Oop *args,size_t count){return receiver;}
static Oop init(const IdProtocol *id,Oop owner,const Oop *symbols,size_t sn,const Oop *args,size_t count){
 api=*id;if(sn!=9||count<1||count>2)return id->invalid;unit=owner;namespace=args[0];
 for(size_t i=0;i<9;i++)keys[i]=symbols[i];
 Oop index=id_size(0);
 Oop method=api.message(id->root,unit,keys[0],&index,1);
#ifdef FAIL_INITIALIZER
 return id_failure(method);
#else
 return method;
#endif
}
static const IdLiteral literals[]={LIT("declaredMethodAt:"),LIT("compiledEcho:"),LIT("at:"),LIT("GC"),LIT("collectPrecisely"),LIT("retirementReady"),LIT("checkpoint"),LIT("initialize"),LIT("withContext:")};
static const IdMethodSpec methods[]={{1,echo},{5,ready},{6,checkpoint}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,9,methods,3,init};
