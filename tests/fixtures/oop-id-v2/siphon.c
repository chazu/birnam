#include "id.h"
#include <stdlib.h>

static Oop initialize(const IdProtocol *id, Oop owner,
                      const Oop *symbols, size_t symbol_count,
                      const Oop *arguments, size_t count) {
    (void)owner;
    (void)symbols;
    (void)symbol_count;
    return count == 1 ? arguments[0] : id->invalid;
}

const IdModule birnam_native_module_v2 = {
    2, sizeof(IdModule), 0, 0, 0, 0, initialize
};

/* The retired initializer must never be used. */
void module_init(void) { abort(); }
