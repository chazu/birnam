#ifndef FIXTURE_NAMESPACE_SUPPORT_H
#define FIXTURE_NAMESPACE_SUPPORT_H
#include "../../abi/fixtures/namespace-v3/namespace_v3.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static void *fixture_namespace_floor;
static NamespaceNamesApiV1 *fixture_names;
static inline void fixture_prepare_namespace(void *floor, const char *floor_path) {
    NamespaceDependencyV3 attach=(NamespaceDependencyV3)dlsym(floor,"birnam_namespace_dependency_v3");
    assert(attach);fixture_namespace_floor=floor;
    const char *slash=strrchr(floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_namespace.dylib",(int)(slash-floor_path),floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=dlopen(path,RTLD_NOW|RTLD_LOCAL); assert(image);
    NamespaceProviderV3 provide=(NamespaceProviderV3)dlsym(image,"birnam_namespace_provider_v3"); assert(provide);
    const NamespaceApiV3 *api=provide(); assert(api && attach(api)==1);
    /* Both handles remain pinned for the process-owned floor. */
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
