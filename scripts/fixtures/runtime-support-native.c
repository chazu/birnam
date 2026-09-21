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
/* Independent row oracle, reached through Id and Bootstrap's admitted factory. */
static void same(DeferredIndexRowV1 a,DeferredIndexRowV1 b) { assert(a.identity==b.identity && a.index==b.index); }
static void check_rows(const DeferredIndexFactoryV1 *f) {
    void *owner=NULL;DeferredIndexRowV1 marker={7,91},out=marker;
    assert(!f->len(owner) && !f->capacity(owner));
    assert(!f->read(owner,0,&out) && !f->write(owner,0,marker) && !f->swap_remove(owner,0,&out));same(out,marker);
    f->clear(owner);f->destroy(owner);assert(!owner);
    DeferredIndexRowV1 rows[128];
    for(size_t i=0;i<128;i++) {
        uint64_t bits=UINT64_C(1)<<(i%64);if(i>=64)bits=~bits;
        rows[i]=(DeferredIndexRowV1){bits,(uintptr_t)~bits};f->push(&owner,rows[i]);
        assert(owner && f->len(owner)==i+1 && f->capacity(owner)>=i+1);
    }
    size_t capacity=f->capacity(owner);void *moved=owner;owner=NULL;
    assert(!f->read(moved,SIZE_MAX,&out) && !f->write(moved,SIZE_MAX,marker) && !f->swap_remove(moved,SIZE_MAX,&out));same(out,marker);
    for(size_t i=0;i<128;i+=3) { rows[i]=marker;assert(f->write(moved,i,marker)==1); }
    for(size_t length=128;length;length--) {
        size_t at=length/3;assert(f->swap_remove(moved,at,&out)==1);same(out,rows[at]);rows[at]=rows[length-1];
        assert(f->len(moved)==length-1 && f->capacity(moved)==capacity);
        for(size_t i=0;i+1<length;i++) { assert(f->read(moved,i,&out)==1);same(out,rows[i]); }
    }
    f->clear(moved);assert(f->capacity(moved)==capacity);
    void *second=NULL;f->push(&second,marker);f->destroy(second);
    f->push(&moved,(DeferredIndexRowV1){0,SIZE_MAX});assert(f->capacity(moved)==capacity);
    assert(f->read(moved,0,&out)==1);same(out,(DeferredIndexRowV1){0,SIZE_MAX});f->destroy(moved);
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

    check_rows(&f->index);

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
