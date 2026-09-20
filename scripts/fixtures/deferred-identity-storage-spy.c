/* Harness-only forwarding library. Every cell operation reaches real Storage. */
#include "storage_v1.h"
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
const DeferredIdentityFactoryV1 *birnam_deferred_identity_factory_v1(void) {
    if (!real_factory) real_factory=((DeferredIdentityFactoryProviderV1)symbol("birnam_deferred_identity_factory_v1"))();
    assert(real_factory && real_factory->version==1 && real_factory->byte_size==sizeof(*real_factory));
    return &spy;
}
/* Additional export belongs only to this fixture; reports the actual Heap cell. */
uint64_t deferred_identity_fixture_writes_v1(void) {
    assert(creates==1 && owner && real_factory->read(owner)==writes);
    if(writes)assert(reads>=writes);return writes;
}
const void *birnam_allocation_identity_factory_v1(void) {
    return ((const void *(*)(void))symbol("birnam_allocation_identity_factory_v1"))();
}
const void *birnam_storage_observation_factory_v1(void) {
    return ((const void *(*)(void))symbol("birnam_storage_observation_factory_v1"))();
}
int birnam_storage_describe_v1(const StorageViewV1 *view,const StorageDescribeRequestV1 *request) {
    return ((StorageDescribeV1)symbol("birnam_storage_describe_v1"))(view,request);
}
void *module_init(IdApi *api,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *roots,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizers) {
    return ((ModuleInitFn)symbol("module_init"))(api,prior,presence,imports,exports,vtables,observation,roots,settings,layouts,hooks,finalizers);
}
int birnam_symbol_borrow_v1(void) {
    return ((int (*)(void))symbol("birnam_symbol_borrow_v1"))();
}
