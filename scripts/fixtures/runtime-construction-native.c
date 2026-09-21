/* Independent C validation of the separate upper Runtime constructor. */
#include "../../abi/fixtures/runtime-support-v1/runtime_support_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>

/* Handwritten C declaration: validation must precede any ownership transfer. */
static void check_runtime_constructor(const BirnamProtocol *id, void *image) {
    typedef void *(*RuntimeV15)(const BirnamProtocol *, const void *, const void *,
        const void *, const void *, const void *, const void *, const void *, const void *, const void *, const void *);
    RuntimeV15 initialize = (RuntimeV15)dlsym(image, "birnam_runtime_init_v15");
    assert(initialize && !dlsym(image, "birnam_runtime_init_v14"));
    assert(!initialize(NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    BirnamProtocol bad = *id;
    bad.version = 99;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    bad = *id;
    bad.byte_size = 8;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    bad = *id;
    bad.message = NULL;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    assert(!initialize(id, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));

}

static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *args, size_t count) {
    assert(count == 2 && args[0].kind == BIRNAM_BYTES && args[1].kind == BIRNAM_BYTES);
    char *path = malloc(args[0].data.bytes.length + 1);
    assert(path);
    memcpy(path, args[0].data.bytes.pointer, args[0].data.bytes.length);
    path[args[0].data.bytes.length] = 0;
    void *image = dlopen(path, RTLD_NOW | RTLD_LOCAL);
    free(path);
    assert(image);
    check_runtime_constructor(id, image);
    const BirnamModule *module = dlsym(image, "birnam_native_module_v1");
    assert(module && module->version == 1 && module->byte_size == sizeof(*module));
    assert(module->initialize(id, NULL, 0).status == BIRNAM_INVALID_REQUEST);
    BirnamOperand invalid[10] = {0};
    assert(module->initialize(id, invalid, 10).status == BIRNAM_INVALID_REQUEST);
    for (size_t i=0; i<10; i++) invalid[i].kind = BIRNAM_WORD;
    assert(module->initialize(id, invalid, 10).status == BIRNAM_INVALID_REQUEST);
    assert(args[1].data.bytes.length == sizeof(invalid));
    memcpy(invalid,args[1].data.bytes.pointer,sizeof(invalid));
    const RuntimeFactoriesV1 *complete=(const void *)invalid[8].data.word;
    assert(complete && complete->version==1 && complete->byte_size==sizeof(*complete));
    for (unsigned fault=0;fault<8;fault++) {
        RuntimeFactoriesV1 bad=*complete;
        switch(fault) {
            case 0: bad.version=99; break;
            case 1: bad.byte_size=16; break;
            case 2: bad.observation.create=NULL; break;
            case 3: bad.storage.advance_epoch=NULL; break;
            case 4: bad.identity.write=NULL; break;
            case 5: bad.index.swap_remove=NULL; break;
            case 6: bad.cache.lookup=NULL; break;
            case 7: bad.verdict.read=NULL; break;
        }
        invalid[8].data.word=(size_t)&bad;
        assert(module->initialize(id,invalid,10).status==BIRNAM_INVALID_REQUEST);
    }
    invalid[8].data.word=(size_t)complete;
    typedef struct { uint32_t version; size_t byte_size; void *integer, *floating, *word, *string; } RuntimeValuesV1;
    _Static_assert(sizeof(RuntimeValuesV1)==48, "value descriptor size");
    const RuntimeValuesV1 *values=(const void *)invalid[9].data.word;
    assert(values && values->version==1 && values->byte_size==sizeof(*values));
    for(unsigned fault=0;fault<6;fault++) {
        RuntimeValuesV1 bad=*values;
        switch(fault) {
            case 0: bad.version=99; break;
            case 1: bad.byte_size=16; break;
            case 2: bad.integer=NULL; break;
            case 3: bad.floating=NULL; break;
            case 4: bad.word=NULL; break;
            case 5: bad.string=NULL; break;
        }
        invalid[9].data.word=(size_t)&bad;
        assert(module->initialize(id,invalid,10).status==BIRNAM_INVALID_REQUEST);
    }
    const char *setters[]={"birnam_method_observation_dependency_v1", "birnam_storage_observation_dependency_v1",
        "birnam_deferred_identity_dependency_v1", "birnam_deferred_index_dependency_v1",
        "birnam_dispatch_cache_dependency_v2", "birnam_guard_verdict_dependency_v1"};
    for(size_t i=0;i<sizeof(setters)/sizeof(setters[0]);i++) assert(!dlsym(image,setters[i]));
    assert(!dlclose(image));
    return (BirnamOutcome){ .status = BIRNAM_LOADED };
}
const BirnamModule birnam_native_module_v1 = {1, sizeof(BirnamModule), initialize};
