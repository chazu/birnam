/* Physical graph fixtures. Weak node observations are paired with identities;
   the declared reference owner supplies graph edges through ordinary messages. */
#include "../oop-id-v2/id.h"
static IdProtocol id;
enum { AT, PUT, COLLECT, RETAIN, RELEASE, IDENTITY, DETAIL, OBSERVE, REPORT,
       ALLOCATION, ROOT, EDGE, COMPLETE, VISIT, REFS, REACHED, UNREACHED,
       CONFIG, FLAGS, STATS, ROW, RUN, COLLECT_NOW, EMPTY, PRESENT, ALLOCS,
       CENSUS, SETTING, PROXY_COMPLETE, OBSERVED, COUNT };
static Oop names[COUNT];
/* Context: Roots,GC,Storage,Trace,report,proxy,observe alias,failure,saved sink,target. */
static Oop nodes[5], identities[5], row_ids[5];
static unsigned seen[5], marked[5];
static size_t samples, rows, completed, observations, gaps, previous, settings;
static int collecting, bad_owner, fail_rows, sweep=1, full, in_collection;
static size_t wanted, alive;
#define CHECK(v) do { Oop checked=(v); if(id_fault(checked))return checked; } while(0)
static Oop send(Oop r,size_t s,const Oop*a,size_t n){return id.message(id.root,r,names[s],a,n);}
static Oop get(Oop c,size_t i){Oop k=id_size(i);return send(c,AT,&k,1);}
static Oop put(Oop c,size_t i,Oop v){Oop a[]={id_size(i),v};return send(c,PUT,a,2);}
static Oop collect(Oop c){in_collection=1;Oop result=send(get(c,1),COLLECT,0,0);in_collection=0;return result;}
static int sample(Oop v){for(size_t i=0;i<samples;i++)if(nodes[i]==v)return (int)i;return -1;}
static int failure_detail(Oop result,size_t expected){
    if(!id_fault(result))return 0;
    Oop detail=send(id_error_value(result),DETAIL,0,0);
    if(id_fault(detail))return 0;
    if((uintptr_t)detail&1)return detail==id_size(expected);
    Oop one=id_size(1);return send(detail,AT,&one,1)==id_size(expected);
}
static Oop config(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n>5)return id.invalid;samples=n;
    for(size_t i=0;i<n;i++){nodes[i]=a[i];identities[i]=send(id.root,IDENTITY,&a[i],1);CHECK(identities[i]);}
    return 0;
}
static Oop flags(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=5)return id.invalid;
    for(size_t i=0;i<n;i++)if(a[i]!=id_size(0)&&a[i]!=id_size(1))return id.invalid;
    collecting=(int)id_unbox(a[0]);bad_owner=(int)id_unbox(a[1]);fail_rows=(int)id_unbox(a[2]);sweep=(int)id_unbox(a[3]);full=(int)id_unbox(a[4]);return 0;
}
static Oop stats(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=1)return id.invalid;
    size_t values[]={rows,completed,observations,gaps,settings};size_t i=id_unbox(a[0]);return i<5?id_size(values[i]):id.invalid;
}
static Oop row_value(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)c;(void)r;if(n!=2)return id.invalid;size_t i=id_unbox(a[0]),field=id_unbox(a[1]);
    if(i>=samples||!seen[i])return id.invalid;
    return field==0?row_ids[i]:field==1?id_size(marked[i]):id.invalid;
}
static Oop observed(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;if(n!=1)return id.invalid;observations++;
    Oop previous_target=get(c,9);CHECK(put(c,9,a[0]));Oop proxy=get(c,5);
    Oop result=id.message(id.root,r,get(c,6),&proxy,1),restored=put(c,9,previous_target);
    CHECK(result);CHECK(restored);return result;
}
static Oop project(Oop c,size_t selector,const Oop*a,size_t n,size_t objects){
    for(size_t i=0;i<objects;i++)if(sample(a[i])<0)return 0;
    return send(get(c,9),selector,a,n);
}
static Oop allocation(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return n==3?project(c,ALLOCATION,a,n,1):id.invalid;}
static Oop root_row(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return n==1?project(c,ROOT,a,n,1):id.invalid;}
static Oop edge(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return n==2?project(c,EDGE,a,n,2):id.invalid;}
static Oop projected_complete(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return n==1?project(c,COMPLETE,a,n,0):id.invalid;}
static Oop empty(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;(void)n;return 0;}
static Oop references(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=2)return id.invalid;
    int i=sample(a[0]);if(i<0)return id.invalid;
    if(send(id.root,IDENTITY,a,1)!=identities[i])return id.invalid;
    if(!in_collection){
        Oop reporter=get(c,4),nested=send(get(c,2),OBSERVE,&reporter,1);
        if(!failure_detail(nested,2))return id.invalid;
        if(bad_owner){
            if(!get(c,8))CHECK(put(c,8,a[1]));
            /* Invalid arity enters the visitor and must latch despite swallowing. */
            if(!id_fault(send(a[1],VISIT,0,0)))return id.invalid;
            return 0;
        }
        if(collecting)CHECK(collect(c));
    }
    if(i<4 && samples>=4){Oop to=nodes[(size_t)i^1];CHECK(send(a[1],VISIT,&to,1));}
    return 0;
}
static Oop report_row(Oop c,const Oop*a,size_t n,int reached){
    if(n!=2)return id.invalid;
    Oop identity=send(id.root,IDENTITY,a,1);CHECK(identity);
    if(identity!=a[1]||id_unbox(identity)<=previous)return id.invalid;
    previous=id_unbox(identity);rows++;
    if(!full){int i=sample(a[0]);if(i<0||seen[i])return id.invalid;seen[i]=1;row_ids[i]=identity;marked[i]=(unsigned)reached;}
    return fail_rows?id_failure(get(c,7)):0;
}
static Oop reached(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return report_row(c,a,n,1);}
static Oop unreached(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;return report_row(c,a,n,0);}
static Oop complete(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=1||completed)return id.invalid;gaps=id_unbox(a[0]);completed++;
    if(!full){Oop args[]={get(c,4),id_size(1)};if(!failure_detail(send(get(c,3),REPORT,args,2),2))return id.invalid;}
    if(collecting){CHECK(collect(c));if(!full)for(size_t i=0;i<samples;i++)if(seen[i]&&send(id.root,IDENTITY,&nodes[i],1)!=row_ids[i])return id.invalid;}
    return id_size(73);
}
static Oop run(Oop root,Oop c,Oop r,const Oop*a,size_t n){
    (void)root;(void)r;if(n!=1)return id.invalid;rows=completed=previous=0;for(size_t i=0;i<5;i++)seen[i]=0;
    if(sweep){
        Oop tokens[5],result=0;size_t kept=0;
        for(size_t i=0;i<samples;i++){result=send(get(c,0),RETAIN,&nodes[i],1);if(id_fault(result))break;tokens[kept++]=result;}
        if(!id_fault(result))result=collect(c);
        while(kept){Oop released=send(get(c,0),RELEASE,&tokens[--kept],1);if(!id_fault(result))result=released;}
        CHECK(result);
    }
    Oop args[]={get(c,4),a[0]};return send(get(c,3),REPORT,args,2);
}
static Oop collect_now(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)r;(void)a;return n?id.invalid:collect(c);}
static Oop census(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;if(n!=3)return id.invalid;if(id_unbox(a[2])==wanted)alive++;return 0;}
static Oop present(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;if(n!=1)return id.invalid;wanted=id_unbox(a[0]);alive=0;CHECK(send(id.root,ALLOCS,&r,1));return id_size(alive);}
static Oop setting(Oop root,Oop c,Oop r,const Oop*a,size_t n){(void)root;(void)c;(void)r;(void)a;(void)n;settings++;return id.invalid;}
static Oop initialize(const IdProtocol*p,Oop image,const Oop*s,size_t count,const Oop*a,size_t n){(void)a;if(n||count!=COUNT)return p->invalid;id=*p;for(size_t i=0;i<count;i++)names[i]=s[i];return image;}
static const IdLiteral literals[]={
 LIT("at:"),LIT("at:put:"),LIT("collectPrecisely"),LIT("retain:"),LIT("release:"),LIT("identityOf:"),LIT("detail"),LIT("observe:"),LIT("report:acceptingGaps:"),
 LIT("allocation:bytes:identity:"),LIT("root:"),LIT("edgeFrom:to:"),LIT("complete:"),LIT("visit:"),LIT("referencesOf:do:"),LIT("reached:identity:"),LIT("notReached:identity:"),
 LIT("configure"),LIT("flags"),LIT("stats"),LIT("row:field:"),LIT("runReport:"),LIT("collectNow"),LIT("empty"),LIT("present:"),LIT("allocationsDo:"),
 LIT("census"),LIT("setting"),LIT("proxyComplete"),LIT("observed")};
static const IdMethodSpec methods[]={
 {CONFIG,config},{FLAGS,flags},{STATS,stats},{ROW,row_value},{RUN,run},{COLLECT_NOW,collect_now},{EMPTY,empty},{PRESENT,present},{CENSUS,census},{SETTING,setting},
 {OBSERVED,observed},{ALLOCATION,allocation},{ROOT,root_row},{EDGE,edge},{PROXY_COMPLETE,projected_complete},{REFS,references},{REACHED,reached},{UNREACHED,unreached},{COMPLETE,complete}};
const IdModule birnam_native_module_v2={2,sizeof(IdModule),literals,COUNT,methods,19,initialize};
