/* Independent declared graphs and native cleanup through Oop messages. */
#include "../oop-id-v2/id.h"
#include <string.h>
static IdProtocol id;
enum { AT, PUT, SEQ, LOW, HIGH, FROM_WORD, FROM_BYTES, COLLECT, IDENTITY,
       ALLOCATION, ROOT, EDGE, COMPLETE, OBSERVE, REACHED, UNREACHED,
       CLEANUP, ECHO, CONFIG, STATS, ROW, RETAIN, RELEASE, COUNT };
static Oop names[COUNT];
/* GC,Trace,Word,String,Bytes,nodes,identities,reporter,saved sink,
   unused,returned Method,row fault,completion fault,cleanup fault,nil,end alias,gaps,Roots. */
static size_t edges,roots,anomaly,mode,result_mode,cleanup_mode,row_count,completed,expected_identity;
static size_t row_nodes[4],row_marks[4];static uint64_t row_ids[4];
static unsigned char bytes[23]="native completion bytes";
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked))return checked; } while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop node(Oop c,size_t i){return get(get(c,5),i);}
static Oop word(Oop c,uint64_t bits){
    if(bits<=UINTPTR_MAX/2)return id_size((size_t)bits);
    Oop a[]={id_size((uint32_t)bits),id_size((size_t)(bits>>32))};return send(get(c,2),FROM_WORD,a,2);
}
static int bits(Oop v,uint64_t*out){
    if((uintptr_t)v&1){*out=id_unbox(v);return 1;}
    Oop lo=send(v,LOW,0,0),hi=send(v,HIGH,0,0);
    if(id_fault(lo)||id_fault(hi)||!((uintptr_t)lo&1)||!((uintptr_t)hi&1)||id_unbox(lo)>UINT32_MAX||id_unbox(hi)>UINT32_MAX)return 0;
    *out=(uint64_t)id_unbox(lo)|((uint64_t)id_unbox(hi)<<32);return 1;
}
static Oop make(Oop c,size_t factory,const unsigned char*data,size_t n){
    if(n>64)return id.invalid;Oop owner=get(c,factory),values[64];
    for(size_t i=0;i<n;i++)values[i]=id_size(data[i]);
    Oop source=send(id.root,SEQ,values,n);CHECK(source);return send(owner,FROM_BYTES,&source,1);
}
static Oop config(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=6)return id.invalid;
    for(size_t i=0;i<n;i++)if(!((uintptr_t)a[i]&1))return id.invalid;
    edges=id_unbox(a[0]);roots=id_unbox(a[1]);anomaly=id_unbox(a[2]);mode=id_unbox(a[3]);result_mode=id_unbox(a[4]);cleanup_mode=id_unbox(a[5]);
    row_count=completed=0;memcpy(bytes,"native completion bytes",sizeof(bytes));return 0;
}
static Oop stats(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1)return id.invalid;
    size_t values[]={row_count,completed,bytes[0],expected_identity};size_t i=id_unbox(a[0]);return i<4?id_size(values[i]):id.invalid;
}
static Oop row(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=2)return id.invalid;size_t i=id_unbox(a[0]),f=id_unbox(a[1]);if(i>=row_count)return id.invalid;
    return f==0?id_size(row_nodes[i]):f==1?word(c,row_ids[i]):f==2?id_size(row_marks[i]):id.invalid;
}
static Oop emit_allocation(Oop c,Oop sink,size_t i,Oop identity){Oop a[]={node(c,i),id_size(0),identity};return send(sink,ALLOCATION,a,3);}
static Oop emit_edge(Oop c,Oop sink,size_t from,size_t to){Oop a[]={node(c,from),node(c,to)};return send(sink,EDGE,a,2);}
static Oop observe(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=1)return id.invalid;Oop sink=a[0];
    if(!get(c,8))CHECK(put(c,8,sink));
    if(mode==1){Oop invalid[]={get(c,1),get(c,1),id_size(9)};if(!id_fault(send(sink,ALLOCATION,invalid,3)))return id.invalid;}
    if(mode==2)return id_size(44);
    if(anomaly!=7){
        Oop ids=get(c,6);
        CHECK(emit_allocation(c,sink,2,anomaly==5?id_size(0):get(ids,2)));
        CHECK(emit_allocation(c,sink,0,get(ids,0)));CHECK(emit_allocation(c,sink,1,get(ids,1)));
        if(anomaly==3)CHECK(emit_allocation(c,sink,3,get(ids,0)));
        if(anomaly==4)CHECK(emit_allocation(c,sink,0,id_size(7)));
        for(size_t i=0;i<3;i++)if(roots&(1U<<i)){Oop value=node(c,i);CHECK(send(sink,ROOT,&value,1));}
        for(size_t i=0;i<3;i++)for(size_t j=0;j<3;j++)if(edges&(1U<<(i*3+j)))CHECK(emit_edge(c,sink,i,j));
        if(anomaly==1){Oop value=node(c,3);CHECK(send(sink,ROOT,&value,1));}
        if(anomaly==2)CHECK(emit_edge(c,sink,2,3));
        if(anomaly==6){Oop value=node(c,0);CHECK(send(sink,ROOT,&value,1));CHECK(send(sink,ROOT,&value,1));CHECK(emit_edge(c,sink,0,1));CHECK(emit_edge(c,sink,0,1));CHECK(emit_edge(c,sink,1,0));}
    }
    Oop gaps=get(c,16),answer=send(sink,COMPLETE,&gaps,1);return mode==3?id_size(44):answer;
}
static Oop report_row(Oop c,const Oop*a,size_t n,size_t marked){
    if(n!=2||row_count>=4)return id.invalid;size_t index=4;
    for(size_t i=0;i<4;i++)if(node(c,i)==a[0])index=i;
    if(index==4||!bits(a[1],&row_ids[row_count]))return id.invalid;
    row_nodes[row_count]=index;row_marks[row_count++]=marked;
    return result_mode==5?id_failure(get(c,11)):0;
}
static Oop reached(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return report_row(c,a,n,1);}
static Oop unreached(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return report_row(c,a,n,0);}
static Oop finish(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;uint64_t actual,expected;if(n!=1||!bits(a[0],&actual)||!bits(get(c,16),&expected)||actual!=expected)return id.invalid;
    completed++;
    if(result_mode==1||result_mode==8)return 0;
    if(result_mode==2||result_mode==3){
        Oop result=result_mode==2?make(c,3,(const unsigned char*)"fresh completion",16):make(c,4,bytes,sizeof(bytes));CHECK(result);
        Oop identity=send(id.root,IDENTITY,&result,1);CHECK(identity);expected_identity=id_unbox(identity);return result;
    }
    if(result_mode==4)return id_failure(get(c,12));
    if(result_mode==6)return get(c,10);
    if(result_mode==7)return id.root;
    if(result_mode==9)return get(c,14);
    if(result_mode==10)return get(c,12);
    return get(get(c,6),2);
}
static Oop cleanup(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;Oop answer=id.message(id.root,r,get(c,15),a,n),value=id_error_value(answer),pin=0;
    if(value){pin=send(get(c,17),RETAIN,&value,1);CHECK(pin);}
    memset(bytes,'!',sizeof(bytes));Oop collected=send(get(c,0),COLLECT,0,0),released=0;
    if(value)released=send(get(c,17),RELEASE,&pin,1);
    CHECK(collected);CHECK(released);
    return cleanup_mode==2?id_failure(get(c,13)):answer;
}
static Oop echo(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1)return id.invalid;
    Oop identity=send(id.root,IDENTITY,a,1);CHECK(identity);expected_identity=id_unbox(identity);return a[0];
}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={LIT("at:"),LIT("at:put:"),LIT("sequence"),LIT("low32"),LIT("high32"),LIT("fromUnsignedLow:high:"),LIT("fromBytes:"),LIT("collectPrecisely"),LIT("identityOf:"),
 LIT("allocation:bytes:identity:"),LIT("root:"),LIT("edgeFrom:to:"),LIT("complete:"),LIT("observe:"),LIT("reached:identity:"),LIT("notReached:identity:"),LIT("cleanup"),LIT("echoBytes:"),LIT("configure"),LIT("stats"),LIT("row:field:"),LIT("retain:"),LIT("release:")};
static const IdMethodSpec methods[]={{OBSERVE,observe},{REACHED,reached},{UNREACHED,unreached},{COMPLETE,finish},{CLEANUP,cleanup},{ECHO,echo},{CONFIG,config},{STATS,stats},{ROW,row}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,9,initialize};
