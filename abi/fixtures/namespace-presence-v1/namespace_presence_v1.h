/* Independent borrowed presence capability; no IdApi dependency. */
#ifndef BIRNAM_NAMESPACE_PRESENCE_V1_H
#define BIRNAM_NAMESPACE_PRESENCE_V1_H
#include <stdint.h>
#include <stddef.h>
typedef int (*NamespacePresenceSlotV1)(const char *);
typedef struct {
    uint32_t version;
    size_t byte_size;
    void *context;
    int (*contains)(void *,const char *);
} NamespacePresenceV1;
_Static_assert(sizeof(NamespacePresenceV1)==32 && offsetof(NamespacePresenceV1,contains)==24,"Namespace presence ABI");
static inline int namespace_presence_invoke_slot_v1(void *context,const char *name) {
    NamespacePresenceSlotV1 selected=*(const NamespacePresenceSlotV1 *)context;
    return selected(name);
}
static inline NamespacePresenceV1 namespace_presence_borrow_slot_v1(const NamespacePresenceSlotV1 *slot) {
    return (NamespacePresenceV1){1,sizeof(NamespacePresenceV1),(void *)slot,namespace_presence_invoke_slot_v1};
}
#endif
