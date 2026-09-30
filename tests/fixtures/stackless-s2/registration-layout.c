#include "registration-envelope-v1.h"

#include <stddef.h>

_Static_assert(sizeof(void *) == 8, "Birnam v1 Oop width");
_Static_assert(sizeof(size_t) == 8, "Birnam v1 size_t width");
_Static_assert(sizeof(BirnamMethodV1) == 8, "Birnam v1 Method width");
_Static_assert(_Alignof(BirnamResumableRegistrationV1) == 8,
               "Birnam registration alignment");
_Static_assert(sizeof(BirnamResumableRegistrationV1) == 136,
               "Birnam registration size");

#define BIRNAM_OFFSET(field, expected)                                      \
    _Static_assert(offsetof(BirnamResumableRegistrationV1, field) == expected, \
                   #field " offset")

BIRNAM_OFFSET(carrier_version, 0x00);
BIRNAM_OFFSET(struct_size, 0x04);
BIRNAM_OFFSET(operation, 0x08);
BIRNAM_OFFSET(flags, 0x0c);
BIRNAM_OFFSET(manifest_entry_index, 0x10);
BIRNAM_OFFSET(reserved0, 0x14);
BIRNAM_OFFSET(registration_token, 0x18);
BIRNAM_OFFSET(descriptor_bytes, 0x20);
BIRNAM_OFFSET(descriptor_length, 0x28);
BIRNAM_OFFSET(entry_thunk, 0x30);
BIRNAM_OFFSET(step_method, 0x38);
BIRNAM_OFFSET(implementation_oop, 0x40);
BIRNAM_OFFSET(code_generation_identity, 0x48);
BIRNAM_OFFSET(code_generation_lease, 0x50);
BIRNAM_OFFSET(portable_implementation_identity, 0x58);
BIRNAM_OFFSET(language_arity, 0x60);
BIRNAM_OFFSET(driver_argument_count, 0x64);
BIRNAM_OFFSET(descriptor_sha256, 0x68);

int main(void) { return 0; }
