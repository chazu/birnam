#include "idapi_v20.h"
#include "reflection-fixture.h"

#include "../../abi/fixtures/idapi-v20/storage_v1.h"
static const VtableEnumerationV2 *enumeration;
static unsigned root_calls;
static int indexed;

static void check(void) {
    if(indexed)assert(vtable_at_v2(enumeration,0)==api->vtable_vt);else assert(vtable_count_v2(enumeration)>0);
}
static void roots(MarkFn mark) { (void)mark;root_calls++;check(); }
static int inspect(void *context,const StorageViewV1 *view) { (void)context;(void)view;check();return STORAGE_OK; }
static BirnamOutcome exercise(const BirnamProtocol *id,const BirnamOperand *args,size_t argc) {
    (void)id;assert(argc==7);reflection_start(args,argc);enumeration=fixture_word(args[4]);
    indexed=!strncmp(mode,"at-",3);
    if(strstr(mode,"observation")) {
        StorageRequestV1 request={1,sizeof(request),NULL,inspect};assert(observe(&request)==STORAGE_OK);
        puts("native enumeration remains callable during storage observation");return (BirnamOutcome){0};
    }
    assert(strstr(mode,"collection"));
    uintptr_t anchor=0;fixture_set_stack_bottom((uintptr_t)&anchor+sizeof(anchor));
    fixture_register_root(roots);(void)api->gc_collect();assert(root_calls);
    puts("enumeration remains callable during root collection");
    return (BirnamOutcome){0};
}

FIXTURE_NATIVE_ENTRY(exercise)
