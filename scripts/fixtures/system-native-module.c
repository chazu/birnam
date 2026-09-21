/* Independent module for the language wrapper around Id's descriptor loader. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include <assert.h>
#include <stdlib.h>
#ifndef MODE
#define MODE 0
#endif
/* This must never be used, including when the common descriptor is absent. */
void *module_init(void) { abort(); }
#if MODE != 4
static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *args, size_t count) {
    assert(id && id->version==1 && id->message);
    if(MODE==1) return (BirnamOutcome){.status=BIRNAM_INITIALIZER_FAILED,.detail=71};
    if(MODE==2) return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=42}};
    if(!count) return (BirnamOutcome){0};
    assert(count==2 && args[0].kind==BIRNAM_OBJECT && args[1].kind==BIRNAM_OBJECT);
    return (BirnamOutcome){.value=args[0]};
}
const BirnamModule birnam_native_module_v1={MODE==3?99:1,sizeof(BirnamModule),initialize};
#endif
