#include "id_protocol.h"
#include <assert.h>
#include <string.h>
#ifdef LOADER_MISSING
int not_a_module(void) { return 1; }
#else
static BirnamOutcome initialize(const BirnamProtocol *id, const BirnamOperand *args, size_t count) {
    for (size_t i=0; i<count; ++i) assert(args[i].kind==BIRNAM_OBJECT);
    if (count==0) return (BirnamOutcome){0};
    if (count==1) return (BirnamOutcome){.value=args[0]};
    if (count==2) {
        const char *name="collectPrecisely";
        BirnamOperand text={.kind=BIRNAM_BYTES,.data.bytes={(const uint8_t *)name,strlen(name)}};
        BirnamOutcome symbol=id->message(id->context,id->receiver,id->intern,&text,1);
        assert(symbol.status==BIRNAM_LOADED && symbol.value.kind==BIRNAM_OBJECT);
        BirnamOutcome collected=id->message(id->context,args[0].data.object,symbol.value.data.object,NULL,0);
        assert(collected.status==BIRNAM_LOADED);
        return (BirnamOutcome){.value=args[1]};
    }
    if (count==3) return (BirnamOutcome){.value={.kind=BIRNAM_WORD,.data.word=17}};
    return (BirnamOutcome){.status=BIRNAM_INITIALIZER_FAILED,.detail=912};
}
#ifdef LOADER_INCOMPATIBLE
const BirnamModule birnam_native_module_v1={99,sizeof(BirnamModule),initialize};
#else
const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),initialize};
#endif
#endif
