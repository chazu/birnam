/* Harness-only forwarding library; all row storage remains in real Storage. */
#include "storage_v1.h"
#include "native-fixture.h"
#include "../../abi/fixtures/deferred-index-v1/deferred_index_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>

static void *real_image,*owner;
static const DeferredIndexFactoryV1 *real_factory;
static DeferredIndexRowV1 *expected;
static size_t length,capacity;
static uint64_t pushes,removes,writes;
static void *symbol(const char *name) {
    if(!real_image) {
        const char *path=getenv("BIRNAM_DEFERRED_REAL_STORAGE");assert(path);
        real_image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(real_image);
    }
    void *result=dlsym(real_image,name);assert(result);return result;
}
static void same(DeferredIndexRowV1 a,DeferredIndexRowV1 b) { assert(a.identity==b.identity && a.index==b.index); }
static void check(const void *context) {
    assert(context==owner);
    assert(real_factory->len(context)==length && real_factory->capacity(context)==capacity);
    if(!owner)assert(!length && !capacity && !pushes);
}
static void all_rows(void) {
    check(owner);
    for(size_t i=0;i<length;i++) { DeferredIndexRowV1 row;assert(real_factory->read(owner,i,&row)==1);same(row,expected[i]); }
}
static void destroy(void *context) {
    check(context);real_factory->destroy(context);free(expected);expected=NULL;owner=NULL;length=capacity=0;
}
static size_t row_len(const void *context) { check(context);return real_factory->len(context); }
static size_t row_capacity(const void *context) { check(context);return real_factory->capacity(context); }
static void push(void **slot,DeferredIndexRowV1 row) {
    check(*slot);void *prior=*slot;size_t before=capacity;
    real_factory->push(slot,row);owner=*slot;assert(owner && (!prior || prior==owner));
    capacity=real_factory->capacity(owner);assert(capacity>=before && capacity>=length+1);
    if(length<before)assert(capacity==before);
    expected=realloc(expected,(length+1)*sizeof(*expected));assert(expected);
    expected[length++]=row;pushes++;all_rows();
}
static int read_row(const void *context,size_t position,DeferredIndexRowV1 *out) {
    check(context);DeferredIndexRowV1 before=*out;
    int result=real_factory->read(context,position,out);assert(result==(position<length));
    if(result)same(*out,expected[position]);else same(*out,before);return result;
}
static int write_row(void *context,size_t position,DeferredIndexRowV1 row) {
    check(context);int result=real_factory->write(context,position,row);assert(result==(position<length));
    if(result) { expected[position]=row;writes++; }all_rows();return result;
}
static int swap_remove(void *context,size_t position,DeferredIndexRowV1 *out) {
    check(context);DeferredIndexRowV1 before=*out;
    int result=real_factory->swap_remove(context,position,out);assert(result==(position<length));
    if(result) { same(*out,expected[position]);length--;expected[position]=expected[length];removes++; }
    else same(*out,before);
    all_rows();return result;
}
static void clear(void *context) { check(context);real_factory->clear(context);length=0;all_rows(); }
static const DeferredIndexFactoryV1 spy={1,sizeof(spy),destroy,row_len,row_capacity,push,read_row,write_row,swap_remove,clear};
/* Fixture-only counters. No row, owner or observer authority is published by Id. */
void deferred_index_fixture_counts_v1(uint64_t *out) { all_rows();out[0]=pushes;out[1]=removes;out[2]=writes; }
/* Forward the common native entry, replacing only this factory result. */
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    const BirnamModule *module=symbol("birnam_native_module_v1");
    BirnamOutcome result=module->initialize(id,args,count);
    if(count==1 && args[0].kind==BIRNAM_BYTES && args[0].data.bytes.length==13 &&
        !memcmp(args[0].data.bytes.pointer,"deferredIndex",13)) {
        assert(result.status==BIRNAM_LOADED);real_factory=(void *)fixture_word(result.value);
        assert(real_factory && real_factory->version==1 && real_factory->byte_size==sizeof(*real_factory));
        result.value.data.word=(uintptr_t)&spy;
    }
    return result;
}
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
int birnam_storage_describe_v1(const StorageViewV1 *view,const StorageDescribeRequestV1 *request) {
    return ((StorageDescribeV1)symbol("birnam_storage_describe_v1"))(view,request);
}
