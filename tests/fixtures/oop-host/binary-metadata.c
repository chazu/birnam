#include "../oop-id-v2/id.h"
static IdProtocol id;
static Oop names[6];
static size_t calls;
static Oop check_string(Oop root,Oop original,Oop receiver,const Oop *args,size_t count) {
    (void)root;
    if(count!=1)return id.invalid;
    calls++;
    Oop size=id.message(id.root,args[0],names[0],0,0);
    if(id_fault(size))return size;
    const unsigned char magic[]="birnam.";
    int binary=id_unbox(size)>=sizeof(magic)-1;
    for(size_t i=0;binary&&i<sizeof(magic)-1;i++) {
        Oop index=id_size(i);
        Oop byte=id.message(id.root,args[0],names[1],&index,1);
        if(id_fault(byte))return byte;
        binary=byte==id_size(magic[i]);
    }
    if(binary)return id.invalid;
    Oop values=id.message(id.root,id.root,names[2],args,count);
    if(id_fault(values))return values;
    Oop invocation[]={receiver,values};
    return id.message(id.root,original,names[3],invocation,2);
}
static Oop observed(Oop root,Oop context,Oop receiver,const Oop*args,size_t count) {
    (void)root;(void)context;(void)receiver;(void)args;
    return count?id.invalid:id_size(calls);
}
static Oop initialize(const IdProtocol *p,Oop image,const Oop *symbols,size_t n,const Oop*a,size_t count) {
    (void)a;if(n!=6||count)return p->invalid;id=*p;
    for(size_t i=0;i<n;i++)names[i]=symbols[i];return image;
}
static const IdLiteral literals[]={LIT("sizeWord"),LIT("atWord:"),LIT("sequence"),LIT("invokeOn:arguments:"),LIT("checkString"),LIT("observed")};
static const IdMethodSpec methods[]={{4,check_string},{5,observed}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,6,methods,2,initialize};
