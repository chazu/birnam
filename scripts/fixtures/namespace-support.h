#ifndef FIXTURE_NAMESPACE_SUPPORT_H
#define FIXTURE_NAMESPACE_SUPPORT_H
#include "../../abi/fixtures/namespace-v8/namespace_v8.h"
#include "vtable-enumeration-support.h"
#include "method-observation-support.h"
#include "root-registry-support.h"
#include "gc-settings-support.h"
#include "payload-layout-support.h"
#include "trace-hooks-support.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_namespace_floor;
static NamespaceNamesApiV1 *fixture_names;
static NamespacePresenceV1 *fixture_presence;
static NamespaceImportV1 *fixture_imports;
static inline NamespaceImportV1 *fixture_namespace_imports(void) {
    if(!fixture_imports) {
        assert(fixture_namespace_floor);
        NamespaceImportProviderV1 provide=(NamespaceImportProviderV1)dlsym(fixture_namespace_floor,"birnam_namespace_import_provider_v1");assert(provide);
        fixture_imports=provide();assert(fixture_imports && fixture_imports->version==1 && fixture_imports->byte_size==sizeof(*fixture_imports));
    }
    return fixture_imports;
}
static inline Oop fixture_namespace_import(const char *name) {
    return namespace_import_v1(fixture_namespace_imports(),name);
}
static NamespaceExportV1 *fixture_exports;
static inline NamespaceExportV1 *fixture_namespace_exports(void) {
    if(!fixture_exports) {
        assert(fixture_namespace_floor);
        NamespaceExportProviderV1 provide=(NamespaceExportProviderV1)dlsym(fixture_namespace_floor,"birnam_namespace_export_provider_v1");assert(provide);
        fixture_exports=provide();assert(fixture_exports && fixture_exports->version==1 && fixture_exports->byte_size==sizeof(*fixture_exports));
    }
    return fixture_exports;
}
static inline Oop fixture_namespace_export(const char *name,Oop value) {
    return namespace_export_v1(fixture_namespace_exports(),name,value);
}
/* Per-caller test loans preserve the independent old callback transcripts. */
typedef Oop (*FixtureImportSlot)(const char *);
static inline Oop fixture_import_slot(void *context,const char *name) {
    FixtureImportSlot selected=*(const FixtureImportSlot *)context;return selected(name);
}
#define FIXTURE_IMPORT(slot) {1,sizeof(NamespaceImportV1),&(slot),fixture_import_slot}
static inline NamespacePresenceV1 *fixture_namespace_presence(void) {
    if(!fixture_presence) {
        assert(fixture_namespace_floor);
        NamespacePresenceProviderV1 provide=(NamespacePresenceProviderV1)dlsym(fixture_namespace_floor,"birnam_namespace_presence_provider_v1");assert(provide);
        fixture_presence=provide();assert(fixture_presence && fixture_presence->version==1 && fixture_presence->byte_size==sizeof(*fixture_presence));
    }
    return fixture_presence;
}
static inline int fixture_namespace_contains(const char *name) {
    return namespace_presence_contains_v1(fixture_namespace_presence(),name);
}
/* Only test fixtures use callback-slot loans to compare old traces. */
typedef int (*FixturePresenceSlot)(const char *);
static inline int fixture_presence_slot(void *context,const char *name) {
    FixturePresenceSlot selected=*(const FixturePresenceSlot *)context;return selected(name);
}
#define FIXTURE_PRESENCE(slot) {1,sizeof(NamespacePresenceV1),&(slot),fixture_presence_slot}

static inline void fixture_prepare_namespace(void *floor, const char *floor_path) {
    NamespaceDependencyV8 attach=(NamespaceDependencyV8)dlsym(floor,"birnam_namespace_dependency_v8");
    assert(attach);fixture_namespace_floor=floor;
    const char *slash=strrchr(floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_namespace.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    NamespaceProviderV8 provide=(NamespaceProviderV8)dlsym(image,"birnam_namespace_provider_v8"); assert(provide);
    const NamespaceApiV8 *api=provide(); assert(api && attach(api)==1);
    /* Both handles remain pinned for the process-owned floor. */
    fixture_prepare_vtable_enumeration(floor,floor_path);
    fixture_prepare_method_observation(floor,floor_path);
    fixture_prepare_root_registry(floor,floor_path);
    fixture_prepare_gc_settings(floor,floor_path);
    fixture_prepare_payload_layout(floor,floor_path);
    fixture_prepare_trace_hooks(floor,floor_path);
}
static inline NamespaceNamesApiV1 *fixture_namespace_names(void) {
    if(!fixture_names) {
        assert(fixture_namespace_floor);
        NamespaceNamesProviderV1 provide=(NamespaceNamesProviderV1)dlsym(fixture_namespace_floor,"birnam_namespace_names_provider_v1");assert(provide);
        fixture_names=provide();assert(fixture_names && fixture_names->version==1 && fixture_names->byte_size==sizeof(*fixture_names));
    }
    return fixture_names;
}
static inline const char *fixture_namespace_name_of(Oop value) {
    NamespaceNamesApiV1 ops=*fixture_namespace_names();return ops.name_of(ops.context,value);
}
static inline void fixture_attach_namespace_names(IdApi *api,void *image) {
    NamespaceNamesDependencyV1 attach=(NamespaceNamesDependencyV1)api->dlsym_fn(image,"birnam_namespace_names_dependency_v1");
    if(attach)assert(attach(fixture_namespace_names())==1);
}
static inline size_t fixture_namespace_count(void) {
    const NamespaceCountPublicationV1 *publication=dlsym(fixture_namespace_floor,"birnam_namespace_count_v1");
    assert(publication && publication->version==1 && publication->byte_size==sizeof(*publication) && publication->entry);
    NamespaceCountV1 selected=*publication->entry;assert(selected.version==1 && selected.byte_size==sizeof(selected) && selected.context && selected.count);
    return selected.count(selected.context);
}
static inline Oop fixture_namespace_name_at(size_t index) {
    const NamespaceNameAtPublicationV1 *publication=dlsym(fixture_namespace_floor,"birnam_namespace_name_at_v1");
    assert(publication && publication->version==1 && publication->byte_size==sizeof(*publication) && publication->entry);
    NamespaceNameAtV1 selected=*publication->entry;assert(selected.version==1 && selected.byte_size==sizeof(selected) && selected.context && selected.name_at);
    return selected.name_at(selected.context,index);
}
#endif
