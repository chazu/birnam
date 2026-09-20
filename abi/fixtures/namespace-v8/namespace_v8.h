#ifndef BIRNAM_NAMESPACE_V8_H
#define BIRNAM_NAMESPACE_V8_H
#include <stdint.h>
#include "../namespace-export-v1/namespace_export_v1.h"
#include "../namespace-import-v1/namespace_import_v1.h"
#include "../namespace-presence-v1/namespace_presence_v1.h"
#include "../namespace-names-v1/namespace_names_v1.h"
#include <stddef.h>
#include "../namespace-enumeration-v1/namespace_enumeration_v1.h"
typedef struct { Oop name, value; } NamespaceBindingV1;
typedef struct { const NamespaceBindingV1 *bindings; size_t count; } NamespaceViewV1;
typedef struct {
    uint32_t version; size_t byte_size;
    void *(*create)(void);
    void (*destroy)(void *);
    Oop (*bind)(void *, Oop, Oop);
    Oop (*find)(const void *, Oop);
    Oop (*reverse)(const void *, Oop);
    size_t (*count)(const void *);
    Oop (*name_at)(const void *, size_t);
    NamespaceViewV1 (*bindings)(const void *);
    NamespaceNamesApiV1 *(*create_names)(void *,Oop (*)(const void *,Oop),const NamespaceNameFloorV1 *);
    void (*destroy_names)(NamespaceNamesApiV1 *);
    NamespaceEnumerationV1 *(*create_enumeration)(const void *,size_t (*)(const void *),Oop (*)(const void *,size_t),const NamespaceEnumerationFloorV1 *);
    void (*destroy_enumeration)(NamespaceEnumerationV1 *);
    int (*lookup)(const void *,Oop (*)(const void *,Oop),void *,Oop (*)(void *,const uint8_t *,size_t),const uint8_t *,size_t,Oop *);
    NamespacePresenceV1 *(*create_presence)(const void *,Oop (*)(const void *,Oop),int (*)(const void *,Oop (*)(const void *,Oop),void *,Oop (*)(void *,const uint8_t *,size_t),const uint8_t *,size_t,Oop *),const NamespacePresenceFloorV1 *);
    void (*destroy_presence)(NamespacePresenceV1 *);
    int (*publish)(void *,Oop (*)(void *,Oop,Oop),void *,Oop (*)(void *,const uint8_t *,size_t),void (*)(void *,Oop,Oop),const uint8_t *,size_t,Oop,Oop *);
    NamespaceImportV1 *(*create_import)(const void *,Oop (*)(const void *,Oop),int (*)(const void *,Oop (*)(const void *,Oop),void *,Oop (*)(void *,const uint8_t *,size_t),const uint8_t *,size_t,Oop *),const NamespaceImportFloorV1 *);
    void (*destroy_import)(NamespaceImportV1 *);
    NamespaceExportV1 *(*create_export)(void *,Oop (*)(void *,Oop,Oop),int (*)(void *,Oop (*)(void *,Oop,Oop),void *,Oop (*)(void *,const uint8_t *,size_t),void (*)(void *,Oop,Oop),const uint8_t *,size_t,Oop,Oop *),const NamespaceExportFloorV1 *);
    void (*destroy_export)(NamespaceExportV1 *);
} NamespaceApiV8;
typedef const NamespaceApiV8 *(*NamespaceProviderV8)(void);
typedef int (*NamespaceDependencyV8)(const NamespaceApiV8 *);
_Static_assert(sizeof(NamespaceApiV8)==176 && offsetof(NamespaceApiV8,create)==16 && offsetof(NamespaceApiV8,bindings)==72, "Namespace ABI");
_Static_assert(offsetof(NamespaceApiV8,publish)==136, "Namespace publication ABI");
_Static_assert(offsetof(NamespaceApiV8,lookup)==112, "Namespace lookup ABI");
_Static_assert(sizeof(NamespaceBindingV1)==16 && sizeof(NamespaceViewV1)==16, "Namespace view ABI");
_Static_assert(offsetof(NamespaceApiV8,create_import)==144 && offsetof(NamespaceApiV8,destroy_import)==152,"Namespace import factory ABI");
_Static_assert(offsetof(NamespaceApiV8,create_export)==160 && offsetof(NamespaceApiV8,destroy_export)==168,"Namespace export factory ABI");
#endif
