
static O initialize(const P*p,O unit,const O*l,size_t literal_count,const O*a,size_t n){
 if(!p||p->version!=2||p->byte_size!=sizeof(P))abort();
 id=*p;if(literal_count!=6||n!=2)return bad();
 memcpy(keys,l,sizeof(keys));ns=a[0];owner=unit;
 Call c={0};Call*prior=active;active=&c;
 roots=raw(&c,ns,"at:",&keys[1],1);
 if(c.failure){active=prior;return c.failure;}
 keep(&c,owner);
