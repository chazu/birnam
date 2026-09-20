/* Independent borrowed presence capability; no IdApi dependency. */
#ifndef BIRNAM_NAMESPACE_PRESENCE_V1_H
#define BIRNAM_NAMESPACE_PRESENCE_V1_H
#include <stdint.h>
#include <stddef.h>
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context;
    int (*contains)(void *,const char *);
} NamespacePresenceV1;
_Static_assert(sizeof(NamespacePresenceV1)==32 && offsetof(NamespacePresenceV1,contains)==24,"Namespace presence ABI");
typedef NamespacePresenceV1 *(*NamespacePresenceProviderV1)(void);
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context;
    void *(*access)(void *);
    void *(*intern)(void *,const uint8_t *,size_t);
} NamespacePresenceFloorV1;
_Static_assert(sizeof(NamespacePresenceFloorV1)==40 && offsetof(NamespacePresenceFloorV1,access)==24 && offsetof(NamespacePresenceFloorV1,intern)==32,"Namespace presence floor ABI");
static inline int namespace_presence_contains_v1(const NamespacePresenceV1 *api,const char *name) {
    NamespacePresenceV1 selected=*api;
    return selected.contains(selected.context,name);
}
#endif
