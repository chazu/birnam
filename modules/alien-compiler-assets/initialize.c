
static R initialize(const P*p,const A*a,size_t n){
 if(!p||p->version!=1||p->byte_size!=sizeof(P)||n<2||a[0].kind!=1||a[1].kind!=1)return bad();
 id=*p;ns=a[0].data.object;owner=a[1].data.object;Call c={0};Call*prior=active;active=&c;
 A key=obj(intern(&c,"Roots"));R rr=raw(&c,ns,"at:",&key,1);if(rr.status||rr.value.kind!=1){active=prior;return bad();}roots=rr.value.data.object;
 keep(&c,owner);
