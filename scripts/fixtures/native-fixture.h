#ifndef BIRNAM_NATIVE_FIXTURE_H
#define BIRNAM_NATIVE_FIXTURE_H
/* Test input decoding and actual Id loading; no alternate construction graph. */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
#include <assert.h>
#include <stdlib.h>
#include <string.h>
static inline void *fixture_word(BirnamOperand value) {
    assert(value.kind==BIRNAM_WORD && value.data.word);return (void *)value.data.word;
}
static inline char *fixture_text(BirnamOperand value) {
    assert(value.kind==BIRNAM_BYTES);
    char *text=malloc(value.data.bytes.length+1);assert(text);
    memcpy(text,value.data.bytes.pointer,value.data.bytes.length);text[value.data.bytes.length]=0;
    return text;
}
static inline void *fixture_load_word(const BirnamProtocol *id,BirnamOperand path) {
    assert(path.kind==BIRNAM_BYTES);
    BirnamOutcome result=id->message(id->context,id->receiver,id->load,&path,1);
    assert(result.status==BIRNAM_LOADED);return fixture_word(result.value);
}
/* Storage observation is a host-idle operation. Return a pinned native entry
 * so the host can run the oracle after the loading message has returned. */
#define FIXTURE_NATIVE_ENTRY(exercise) \
    static BirnamOutcome fixture_run(void *context,BirnamObject receiver,const BirnamOperand *args,size_t count) { \
        (void)receiver;return exercise(context,args,count); \
    } \
    static BirnamOutcome fixture_initialize(const BirnamProtocol *id,const BirnamOperand *args,size_t count) { \
        (void)args;assert(count==0);static BirnamProtocol selected;selected=*id; \
        return (BirnamOutcome){.status=BIRNAM_LOADED,.value={.kind=BIRNAM_NATIVE_ENTRY, \
            .data.entry={fixture_run,&selected}}}; \
    } \
    const BirnamModule birnam_native_module_v1={1,sizeof(BirnamModule),fixture_initialize};
#endif
