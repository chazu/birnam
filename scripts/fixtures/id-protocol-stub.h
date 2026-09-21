/* Admission-only fixture: let host tests reach a rejected upper dependency.
 * This is never used as evidence of Id construction or language execution.
 */
#include "../../abi/fixtures/id-protocol-v1/id_protocol.h"
static char admission_tokens[3];
static BirnamOutcome admission_message(void *context, BirnamObject receiver,
    BirnamObject selector, const BirnamOperand *args, size_t count) {
    (void)context;(void)receiver;(void)selector;(void)args;(void)count;
    return (BirnamOutcome){ .status = BIRNAM_UNBOUND };
}
static const BirnamProtocol admission_protocol = {
    1, sizeof(BirnamProtocol), admission_message, admission_tokens,
    &admission_tokens[0], &admission_tokens[1], &admission_tokens[2]
};
static BirnamOutcome admission_initialize(const BirnamProtocol *prior,
    const BirnamOperand *args, size_t count) {
    (void)prior;(void)args;(void)count;
    return (BirnamOutcome){ .value = { .kind = BIRNAM_PROTOCOL,
        .data.protocol = &admission_protocol } };
}
const BirnamModule birnam_native_module_v1 = {
    1, sizeof(BirnamModule), admission_initialize
};
