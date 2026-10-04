
static R invoke(void*unused,O receiver,const A*a,size_t n){
 Call c={0}; Call*prior=active;active=&c;begin(&c);keep(&c,receiver);
 O *args=calloc(n?n:1,sizeof(O));if(!args)c.failure=bad();
 for(size_t i=0;i<n&&!c.failure.status;i++)args[i]=keep(&c,object(&c,a[i]));
 O result=c.failure.status?0:__shim(receiver,args,n);free(args);active=prior;return finish(&c,obj(result));
}
