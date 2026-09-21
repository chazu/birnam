/* Harness-only forwarding library. Every cell operation reaches real Storage. */
#include "storage_v1.h"
#include "native-fixture.h"
#include "../../abi/fixtures/deferred-identity-v1/deferred_identity_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>

static void *real_image, *owner;
static const DeferredIdentityFactoryV1 *real_factory;
static uint64_t reads, writes;
static unsigned creates;
static void *symbol(const char *name) {
    if (!real_image) {
        const char *path=getenv("BIRNAM_DEFERRED_REAL_STORAGE");assert(path);
        real_image=dlopen(path,RTLD_NOW|RTLD_LOCAL);assert(real_image);
    }
    void *result=dlsym(real_image,name);assert(result);return result;
}
static void *create(void) {
    assert(!owner && !creates && !reads && !writes);
    owner=real_factory->create();assert(owner && real_factory->read(owner)==0);creates++;
    return owner;
}
static void destroy(void *context) {
    assert(context==owner);real_factory->destroy(context);owner=NULL;
}
static uint64_t read_cell(const void *context) {
    assert(context==owner);reads++;return real_factory->read(context);
}
static void write_cell(void *context,uint64_t value) {
    assert(context==owner && value==real_factory->read(context)+1);
    real_factory->write(context,value);writes++;assert(real_factory->read(context)==writes);
}
static const DeferredIdentityFactoryV1 spy={1,sizeof(spy),create,destroy,read_cell,write_cell};
/* Additional export belongs only to this fixture; reports the actual Heap cell. */
uint64_t deferred_identity_fixture_writes_v1(void) {
    assert(creates==1 && owner && real_factory->read(owner)==writes);
    if(writes)assert(reads>=writes);return writes;
}
/* Forward the common native entry, replacing only this factory result. */
static BirnamOutcome initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) {
    const BirnamModule *module=symbol("birnam_native_module_v1");
    BirnamOutcome result=module->initialize(id,args,count);
    if(count==1 && args[0].kind==BIRNAM_BYTES && args[0].data.bytes.length==16 &&
        !memcmp(args[0].data.bytes.pointer,"deferredIdentity",16)) {
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
