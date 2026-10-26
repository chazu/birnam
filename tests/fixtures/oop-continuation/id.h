#ifndef OOP_CONTINUATION_ID_H
#define OOP_CONTINUATION_ID_H
/* Independent C declaration: no generated Rust headers or carrier structs. */
#include <stdint.h>
#include <stddef.h>
typedef void *Oop;
typedef Oop (*IdMethod)(Oop id,Oop context,Oop receiver,const Oop *arguments,size_t count);
typedef Oop (*IdSend)(Oop id,Oop receiver,Oop selector,const Oop *arguments,size_t count);
typedef struct IdProtocol {
 uint32_t version,byte_size;
 IdSend message;
 Oop root,intern,load,at,seeds,invalid;
} IdProtocol;
typedef struct IdLiteral {const unsigned char *bytes;size_t length;} IdLiteral;
typedef Oop (*IdInitialize)(const IdProtocol *,Oop image,const Oop *symbols,size_t symbol_count,const Oop *arguments,size_t count);
typedef struct IdMethodSpec {size_t name;IdMethod entry;} IdMethodSpec;
typedef struct IdModule {
 uint32_t version,byte_size;
 const IdLiteral *literals;size_t literal_count;
 const IdMethodSpec *methods;size_t method_count;
 IdInitialize initialize;
} IdModule;
typedef struct IdSeed {const unsigned char *bytes;size_t length;} IdSeed;
_Static_assert(sizeof(Oop)==8,"64-bit Oop");
_Static_assert(sizeof(IdProtocol)==64,"independent protocol layout");
_Static_assert(sizeof(IdModule)==48,"independent module layout");
static inline Oop id_size(size_t n){return (Oop)((n<<1)|1);}
static inline size_t id_unbox(Oop o){return (size_t)o>>1;}
static inline int id_fault(Oop o){return ((uintptr_t)o&3)==2;}
/* The marker belongs to a failing return, not to the allocated object. */
static inline Oop id_error_value(Oop result){return id_fault(result)?(Oop)((uintptr_t)result&~(uintptr_t)2):result;}
static inline Oop id_failure(Oop value){return (Oop)((uintptr_t)value|2);}
#define LIT(s) {(const unsigned char *)(s),sizeof(s)-1}
/* Public v3 initializer handoff. Independent from Rust declarations. */
typedef Oop (*IdControl)(Oop,Oop,const Oop*,size_t);
typedef struct IdContinuationProtocol {IdProtocol common;IdControl control;} IdContinuationProtocol;
typedef Oop (*IdContinuationInitialize)(const IdContinuationProtocol*,Oop,const Oop*,size_t,const Oop*,size_t);
typedef struct IdContinuationMethodSpec {size_t name;IdMethod entry;uint32_t convention;} IdContinuationMethodSpec;
_Static_assert(sizeof(IdContinuationMethodSpec)==24,"independent v3 method layout");
typedef struct IdContinuationModule {
 uint32_t version,byte_size;const IdLiteral *literals;size_t literal_count;
 const IdContinuationMethodSpec *methods;size_t method_count;IdContinuationInitialize initialize;
} IdContinuationModule;
_Static_assert(sizeof(IdContinuationProtocol)==72,"independent continuation protocol layout");
_Static_assert(sizeof(IdContinuationModule)==48,"independent continuation descriptor layout");

#endif
