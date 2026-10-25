
static O invoke(O id_root,O context,O receiver,const O*args,size_t n){
 Call c={0};Call*prior=active;active=&c;begin(&c);keep(&c,receiver);
 for(size_t i=0;i<n&&!c.failure;i++)keep(&c,args[i]);
 O result=c.failure?0:__shim(receiver,(O*)args,n);
 active=prior;return finish(&c,result);
}
