/* Independent C caller for every mounted row operation, including repair writes. */
#include "../../abi/fixtures/deferred-index-v1/deferred_index_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdio.h>

static void same(DeferredIndexRowV1 a,DeferredIndexRowV1 b) { assert(a.identity==b.identity && a.index==b.index); }
int main(int argc,char **argv) {
    assert(argc==2);
    void *image=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(image);
    DeferredIndexFactoryProviderV1 provide=(DeferredIndexFactoryProviderV1)dlsym(image,"birnam_deferred_index_factory_v1");assert(provide);
    const DeferredIndexFactoryV1 *f=provide();assert(f && f->version==1 && f->byte_size==sizeof(*f));
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
    puts("deferred-index C ABI: lazy rows, all bits, misses, unconditional writes, exact swap removal and moved ownership passed");
}
