/* Observe the actual bootstrap Heap owner without adding a production getter. */
#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include "../../abi/fixtures/allocation-budget-v1/allocation_budget_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>

static const AllocationBudgetFactoryV1 *original;
static AllocationBudgetFactoryV1 observed;
static void *heap_budget;
static unsigned creates;
static void *capture_create(void) {
    void *result=original->create();assert(result && !creates++);
    heap_budget=result;return result;
}
static const AllocationBudgetFactoryV1 *provide_observed(void) { return &observed; }
static void *capture_dlsym(void *image,const char *name) {
    void *symbol=dlsym(image,name);
    if(strcmp(name,"birnam_allocation_budget_factory_v1"))return symbol;
    assert(symbol);
    original=((AllocationBudgetFactoryProviderV1)symbol)();
    assert(original && original->version==1 && original->byte_size==sizeof(*original));
    observed=*original;observed.create=capture_create;
    return (void *)provide_observed;
}
/* Only creation is intercepted, before the one immutable attachment. The
 * observer invokes no Id callbacks, including while Heap is borrowed. */
#define dlsym capture_dlsym
#include "namespace-support.h"
#undef dlsym
#include "trace-support.h"

static IdApi *api;
static Oop retained;
static unsigned collections;
static size_t callback_budget;
static void roots(MarkFn mark) {
    collections++;callback_budget=original->read(heap_budget);
    mark(retained);assert(api->gc_collect()==0);
}
int main(int argc,char **argv) {
    assert(argc==3);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);assert(creates==0);
    api=((IdInitFn)dlsym(floor,"id_init"))();assert(api && creates==1);
    assert(original->read(heap_budget)>0);
    Dl_info owner;
    assert(dladdr((void *)original->charge,&owner) && strstr(owner.dli_fname,"libbirnam_allocation_budget.dylib"));
    if(!strcmp(argv[2],"overflow")) {
        original->replace(heap_budget,SIZE_MAX);
        api->alloc(api->object_vt,0);assert(!"overflow returned");
    }
    assert(!strcmp(argv[2],"normal"));
    fixture_set_auto(0);fixture_set_precise(1);
    fixture_mount_trace(api,argv[1],fixture_root_registry());fixture_register_root(roots);
    size_t before=original->read(heap_budget);
    retained=api->alloc(api->object_vt,9);
    assert(original->read(heap_budget)==before+24 && creates==1);
    puts("bootstrap Heap owns one loaded budget; allocation charges its padded extent");
    native_stack_set_v1(fixture_native_stack(),0);
    api->alloc(api->object_vt,16u<<20);fixture_set_auto(1);
    before=original->read(heap_budget);
    api->alloc(api->object_vt,0);
    assert(!collections && original->read(heap_budget)==before+8);
    uintptr_t anchor=0;native_stack_set_v1(fixture_native_stack(),(uintptr_t)&anchor);
    before=original->read(heap_budget);assert(!collections);
    api->alloc(api->object_vt,0);
    assert(collections==1 && callback_budget==before && original->read(heap_budget)==8);
    assert(api->vtable_of(retained)==api->object_vt);
    puts("suppression retains charges; the next admitted allocation collects before charging");
    api->gc_collect();
    assert(collections==2 && callback_budget==8 && original->read(heap_budget)==0);
    assert(api->vtable_of(retained)==api->object_vt);
    puts("root callbacks see the prior charge; collection resets at its original completion point");
}
