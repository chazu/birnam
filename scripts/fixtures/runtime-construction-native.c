/* Independent C validation of the separate upper Runtime constructor. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include <assert.h>
#include <dlfcn.h>
#include <stdlib.h>
#include <string.h>

/* Handwritten C declaration: validation must precede any ownership transfer. */
static void check_runtime_constructor(const BirnamProtocol *id, void *image) {
    typedef void *(*RuntimeV13)(const BirnamProtocol *, const void *, const void *,
        const void *, const void *, const void *, const void *, const void *, const void *);
    RuntimeV13 initialize = (RuntimeV13)dlsym(image, "birnam_runtime_init_v13");
    assert(initialize && !dlsym(image, "birnam_runtime_init_v12"));
    assert(!initialize(NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    BirnamProtocol bad = *id;
    bad.version = 99;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    bad = *id;
    bad.byte_size = 8;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    bad = *id;
    bad.message = NULL;
    assert(!initialize(&bad, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));
    assert(!initialize(id, NULL, NULL, NULL, NULL, NULL, NULL, NULL, NULL));

}

static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *args, size_t count) {
    assert(count == 1 && args[0].kind == BIRNAM_BYTES);
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
    BirnamOperand invalid[8] = {0};
    assert(module->initialize(id, invalid, 8).status == BIRNAM_INVALID_REQUEST);
    for (size_t i=0; i<8; i++) invalid[i].kind = BIRNAM_WORD;
    assert(module->initialize(id, invalid, 8).status == BIRNAM_INVALID_REQUEST);
    assert(!dlclose(image));
    return (BirnamOutcome){ .status = BIRNAM_LOADED };
}
const BirnamModule birnam_native_module_v1 = {1, sizeof(BirnamModule), initialize};
