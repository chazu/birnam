#ifndef BIRNAM_ID_PROTOCOL_V1_H
#define BIRNAM_ID_PROTOCOL_V1_H
#include <stddef.h>
#include <stdint.h>
/* Frozen independent C declaration of Design 83's native boundary. */
typedef void *BirnamObject;
typedef struct BirnamOperand BirnamOperand;
typedef struct BirnamOutcome BirnamOutcome;
typedef struct BirnamProtocol BirnamProtocol;
typedef BirnamOutcome (*BirnamNativeMethod)(void *, BirnamObject, const BirnamOperand *, size_t);
typedef BirnamOutcome (*BirnamMessage)(void *, BirnamObject, BirnamObject, const BirnamOperand *, size_t);
typedef BirnamOutcome (*BirnamInitialize)(const BirnamProtocol *, const BirnamOperand *, size_t);
typedef struct { BirnamNativeMethod method; void *context; } BirnamNativeEntry;
typedef struct { const uint8_t *pointer; size_t length; } BirnamBytes;
enum { BIRNAM_UNIT=0, BIRNAM_OBJECT=1, BIRNAM_WORD=2, BIRNAM_BYTES=3, BIRNAM_NATIVE_ENTRY=4, BIRNAM_PROTOCOL=5 };
enum { BIRNAM_LOADED=0, BIRNAM_INVALID_REQUEST=1, BIRNAM_MAPPING_FAILED=2, BIRNAM_MISSING_ENTRY=3, BIRNAM_INCOMPATIBLE_ABI=4, BIRNAM_INITIALIZER_FAILED=5, BIRNAM_UNBOUND=6 };
struct BirnamOperand {
    uint32_t kind, reserved;
    union { BirnamObject object; size_t word; BirnamBytes bytes; BirnamNativeEntry entry; const BirnamProtocol *protocol; } data;
};
struct BirnamOutcome { uint32_t status, reserved; size_t detail; BirnamOperand value; };
struct BirnamProtocol {
    uint32_t version, byte_size;
    BirnamMessage message;
    void *context;
    BirnamObject receiver, intern, load;
};
typedef struct { uint32_t version, byte_size; BirnamInitialize initialize; } BirnamModule;
_Static_assert(sizeof(BirnamNativeEntry)==16 && offsetof(BirnamNativeEntry,context)==8,"native entry layout");
_Static_assert(sizeof(BirnamOperand)==24 && offsetof(BirnamOperand,data)==8,"operand layout");
_Static_assert(sizeof(BirnamOutcome)==40 && offsetof(BirnamOutcome,detail)==8 && offsetof(BirnamOutcome,value)==16,"outcome layout");
_Static_assert(sizeof(BirnamProtocol)==48 && offsetof(BirnamProtocol,message)==8 && offsetof(BirnamProtocol,context)==16 && offsetof(BirnamProtocol,receiver)==24 && offsetof(BirnamProtocol,intern)==32 && offsetof(BirnamProtocol,load)==40,"protocol layout");
_Static_assert(sizeof(BirnamModule)==16 && offsetof(BirnamModule,initialize)==8,"module layout");
#endif
