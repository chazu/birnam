/* Producer proves an uncaptured, allocation-free argument/alias return.
   Execution owns current Method selection, frame admission and baseline fallback.
   These guards establish the meanings of omitted index construction and lookup. */
static R try_inline_return(const A *a,size_t n,size_t expected,size_t operand){
 if(n!=expected)return ok((A){0});
 for(size_t j=0;j<n;j++)if(a[j].kind!=1)return ok((A){0});
 Call c={0};
 A nsarg=obj(ns);
 A lookup=send(&c,id.receiver,"usesDefaultLookup:",&nsarg,1);
 if(lookup.kind!=2||lookup.data.word!=1)return finish(&c,(A){0});
 A binding[2]={obj(selector(&c,"at:")),obj(ns)};
 A selected=send(&c,id.receiver,"bind:to:",binding,2);
 A meaning=send(&c,ns,"isCanonicalLookup:",&selected,1);
 if(meaning.kind!=2||meaning.data.word!=1)return finish(&c,(A){0});
 A epoch=send(&c,id.receiver,"behaviorEpoch",0,0);
 O integers=named(&c,"Integer");
 R permit=raw(&c,integers,"canOmitIndexing",0,0);
 if(permit.status==6){c.failure=(R){0};return finish(&c,(A){0});}
 if(permit.status||permit.value.kind!=2||permit.value.data.word!=1)
  return finish(&c,(A){0});
 A after=send(&c,id.receiver,"behaviorEpoch",0,0);
 if(epoch.kind!=2||after.kind!=2||epoch.data.word!=after.data.word)
  return finish(&c,(A){0});
 if(named(&c,"Integer")!=integers)return finish(&c,(A){0});
 /* Cleanup can invoke replaced root methods. Afterwards, use only retained
    selector identities and Id-only default fact reads; do not replay effects. */
 O epoch_sel=selector(&c,"behaviorEpoch");
 O at_sel=selector(&c,"at:");
 O int_name=selector(&c,"Integer");
 R clean=finish(&c,(A){0});
 if(clean.status)return clean;
 R fence=id.message(id.context,id.receiver,epoch_sel,0,0);
 if(fence.status)return fence;
 if(fence.value.kind!=2||fence.value.data.word!=epoch.data.word)return ok((A){0});
 A key=obj(int_name);
 R factory=id.message(id.context,ns,at_sel,&key,1);
 if(factory.status)return factory;
 if(factory.value.kind!=1||factory.value.data.object!=integers)return ok((A){0});
 return ok(a[operand]);
}
