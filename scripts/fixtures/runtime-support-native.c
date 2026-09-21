/* Every advertised factory operation works with no Runtime image available. */
#include "../../abi/fixtures/runtime-support-v1/runtime_support_v1.h"
#include <assert.h>
#include <string.h>
static size_t notifications;
static void admit(void *context) { (*(size_t *)context)++; }
static void notify(Oop object, Oop selector) { assert(object && selector); notifications++; }
static BirnamOutcome echo(void *context, BirnamObject receiver, const BirnamOperand *args, size_t count) {
    (void)receiver; (void)args; assert(count==0);
    return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=*(size_t *)context}};
}
static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *args, size_t count) {
    assert(count==1 && args[0].kind==BIRNAM_WORD);
    const RuntimeFactoriesV1 *f=(const void *)args[0].data.word;
    assert(f && f->version==1 && f->byte_size==sizeof(*f));
    size_t admissions=0;
    MethodObservationFloorV1 access={1,sizeof(access),&admissions,admit};
    assert(!f->observation.create(NULL));
    MethodObservationV1 *observer=f->observation.create(&access);
    assert(observer && admissions==0);
    observer->replace(observer->context,notify); assert(admissions==1);
    size_t before=notifications;
    f->observation.notify(observer,0,id->receiver,id->intern); assert(notifications==before);
    f->observation.notify(observer,1,id->receiver,id->intern); assert(notifications==before+1);
    observer->replace(observer->context,NULL); assert(admissions==2);
    f->observation.notify(observer,1,id->receiver,id->intern); assert(notifications==before+1);
    f->observation.destroy(observer);

    void *scope=f->storage.create(), *other=f->storage.create(); assert(scope && other && scope!=other);
    StorageObservationStateV1 state={0}; f->storage.read(scope,&state);
    assert(state.epoch==0 && !state.active && !state.permit);
    f->storage.set_active(scope,1); f->storage.set_permit(scope,1);
    f->storage.read(scope,&state); assert(state.active && state.permit);
    f->storage.read(other,&state); assert(!state.active && !state.permit);
    f->storage.set_active(scope,0); f->storage.set_permit(scope,0);
    uint64_t epoch=0; assert(f->storage.advance_epoch(scope,&epoch)==1 && epoch==1);
    f->storage.read(scope,&state); assert(state.epoch==1 && !state.active && !state.permit);
    f->storage.destroy(scope); f->storage.destroy(other);

    void *identity=f->identity.create(); assert(identity);
    uint64_t initial=f->identity.read(identity);
    f->identity.write(identity,83); assert(f->identity.read(identity)==83);
    f->identity.write(identity,initial); assert(f->identity.read(identity)==initial);
    f->identity.destroy(identity);

    void *rows=NULL; DeferredIndexRowV1 row={0};
    assert(f->index.len(rows)==0 && f->index.capacity(rows)==0);
    assert(!f->index.read(rows,0,&row));
    f->index.push(&rows,(DeferredIndexRowV1){83,4});
    f->index.push(&rows,(DeferredIndexRowV1){84,7});
    assert(f->index.len(rows)==2 && f->index.capacity(rows)>=2);
    assert(f->index.read(rows,0,&row) && row.identity==83 && row.index==4);
    assert(f->index.write(rows,0,(DeferredIndexRowV1){85,9}));
    assert(f->index.swap_remove(rows,0,&row) && row.identity==85 && row.index==9);
    assert(f->index.read(rows,0,&row) && row.identity==84 && row.index==7);
    f->index.clear(rows); assert(f->index.len(rows)==0); f->index.destroy(rows);

    void *cache=f->cache.create(); assert(cache);
    size_t context=83; BirnamNativeEntry entry={echo,&context};
    assert(!f->cache.lookup(cache,3,5,7).method);
    f->cache.insert(cache,3,5,7,entry);
    BirnamNativeEntry selected=f->cache.lookup(cache,3,5,7);
    assert(selected.method==echo && selected.context==&context);
    assert(selected.method(selected.context,id->receiver,NULL,0).value.data.word==83);
    assert(!f->cache.lookup(cache,4,5,7).method);
    f->cache.clear(cache); assert(!f->cache.lookup(cache,3,5,7).method); f->cache.destroy(cache);

    void *verdict=f->verdict.create(); assert(verdict);
    GuardVerdictStateV1 fact={0}; f->verdict.read(verdict,&fact);
    assert(fact.epoch==UINT64_MAX && !fact.verified);
    f->verdict.write(verdict,83,1); f->verdict.read(verdict,&fact); assert(fact.epoch==83 && fact.verified);
    f->verdict.write(verdict,84,0); f->verdict.read(verdict,&fact); assert(fact.epoch==84 && !fact.verified);
    f->verdict.destroy(verdict);
    return (BirnamOutcome){.status=BIRNAM_LOADED};
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
