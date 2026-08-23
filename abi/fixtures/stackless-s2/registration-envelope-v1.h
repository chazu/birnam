#ifndef BIRNAM_REGISTRATION_ENVELOPE_V1_H
#define BIRNAM_REGISTRATION_ENVELOPE_V1_H

#include <stddef.h>
#include <stdint.h>

typedef void *BirnamOopV1;
typedef BirnamOopV1 (*BirnamMethodV1)(
    BirnamOopV1 receiver,
    BirnamOopV1 const *arguments,
    size_t argument_count);
typedef BirnamMethodV1 BirnamStepMethodV1;

enum {
    BIRNAM_REGISTRATION_CARRIER_VERSION_V1 = 1,
    BIRNAM_REGISTRATION_PREPARE_V1 = 1,
    BIRNAM_REGISTRATION_ABORT_V1 = 2,
    BIRNAM_REGISTRATION_OK_V1 = 0,
    BIRNAM_REGISTRATION_REJECTED_V1 = 1,
};

typedef struct BirnamResumableRegistrationV1 {
    uint32_t carrier_version;
    uint32_t struct_size;
    uint32_t operation;
    uint32_t flags;
    uint32_t manifest_entry_index;
    uint32_t reserved0;
    uint64_t registration_token;
    uint8_t const *descriptor_bytes;
    uint64_t descriptor_length;
    BirnamMethodV1 entry_thunk;
    BirnamStepMethodV1 step_method;
    BirnamOopV1 implementation_oop;
    uint64_t code_generation_identity;
    uint64_t code_generation_lease;
    uint64_t portable_implementation_identity;
    uint32_t language_arity;
    uint32_t driver_argument_count;
    uint8_t descriptor_sha256[32];
} BirnamResumableRegistrationV1;

uint32_t birnam_register_resumable_v1(
    BirnamResumableRegistrationV1 *envelope);

#endif
