#ifndef FIXTURE_INTEGER_SUPPORT_H
#define FIXTURE_INTEGER_SUPPORT_H
#include "../../abi/fixtures/integer-v1/integer_v1.h"
#include "float-support.h"
#include "word-support.h"
#include "string-support.h"
#include "../../abi/fixtures/range-methods-v1/range_methods_v1.h"
#include "../../abi/fixtures/range-v1/range_v1.h"
#include "../../abi/fixtures/range-v2/range_v2.h"
#include "../../abi/fixtures/vector-v2/vector_v2.h"
#include "../../abi/fixtures/map-v2/map_v2.h"
#include "../../abi/fixtures/string-collection-v1/string_collection_v1.h"
#include "../../abi/fixtures/collection-value-v1/collection_value_v1.h"
#include <assert.h>
#include <dlfcn.h>
#include <limits.h>
#include <stdio.h>
#include <string.h>
static IntegerApiV1 *fixture_integer;
static char fixture_floor_path[PATH_MAX];
static const RangeMethodsApiV1 *fixture_range_methods; /* Historical 81Z oracle only. */
static const RangeApiV1 *fixture_range; /* Historical 81AA/AB oracle only. */
static const RangeApiV2 *fixture_range_v2;
static const VectorApiV1 *fixture_vector_v1; /* Historical storage oracle only. */
static const VectorApiV2 *fixture_vector_v2;
/* Test-only normalized view; production no longer exposes count/at. */
typedef struct {
    Oop (*empty)(void);
    Oop (*push)(Oop,Oop);
    size_t (*count)(Oop);
    Oop (*at)(Oop,size_t);
    Oop (*behavior)(void);
} FixtureVectorOps;
static FixtureVectorOps fixture_vector_ops;
static const FixtureVectorOps *fixture_vector;
typedef struct { size_t count,shift; Oop root,tail; size_t tail_len; } FixtureVectorPayload;
_Static_assert(sizeof(FixtureVectorPayload)==40, "test Vector payload");
static inline size_t fixture_vector_count(Oop object) { return ((FixtureVectorPayload *)object)->count; }
static inline Oop fixture_vector_at(Oop object,size_t index) {
    FixtureVectorPayload *v=object;
    if(index>=v->count) return NULL;
    Oop node=v->tail;
    if(index<v->count-v->tail_len) {
        node=v->root;
        for(size_t level=v->shift;level;level-=5) {
            node=((Oop *)node)[(index>>level)&31];
            if(!node) return NULL;
        }
    }
    return ((Oop *)node)[index&31];
}
static inline void fixture_prepare_vector(IdApi *api);
static const CollectionValueApiV1 *fixture_collection_value;
static const MapApiV1 *fixture_map; /* Historical storage oracle only. */
static const MapApiV2 *fixture_map_v2;
static inline void fixture_prepare_map(IdApi *api);
static const StringCollectionApiV1 *fixture_string_collection;
static inline void fixture_prepare_string_collection(IdApi *api);
static inline Oop fixture_integer_make(int64_t value) {
    assert(fixture_integer);
    return fixture_integer->make(fixture_integer->context, value);
}
static inline int64_t fixture_integer_value(Oop value) {
    assert(fixture_integer);
    return fixture_integer->read(fixture_integer->context, value);
}
static inline int fixture_is_integer(Oop value) {
    assert(fixture_integer);
    return fixture_integer->is_exact(fixture_integer->context, value);
}
static inline void fixture_attach_integer(IdApi *api, void *image) {
    fixture_attach_string(api, image);
    fixture_attach_float(api, image);
    fixture_attach_word(api, image);
    IntegerDependencyV1 attach = (IntegerDependencyV1)api->dlsym_fn(image, "birnam_integer_dependency_v1");
    if (attach) { assert(fixture_integer); assert(attach(fixture_integer) == 1); }
    CollectionValueDependencyV1 value_attach=(CollectionValueDependencyV1)api->dlsym_fn(image,"birnam_collection_value_dependency_v1");
    if (value_attach) {
        if (!fixture_collection_value) {
            const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
            int n=snprintf(path,sizeof(path),"%.*s/libbirnam_collection_value.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
            assert(n>0 && (size_t)n<sizeof(path));
            void *value=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(value); fixture_attach_integer(api,value);
            ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(value,"module_init"); assert(init);
            CollectionValueProviderV1 provide=(CollectionValueProviderV1)api->dlsym_fn(value,"birnam_collection_value_provider_v1"); assert(provide);
            assert(!provide()); init(api,NULL); fixture_collection_value=provide(); assert(fixture_collection_value);
        }
        assert(value_attach(fixture_collection_value)==1);
    }
    VectorDependencyV2 vector_v2_attach=(VectorDependencyV2)api->dlsym_fn(image,"birnam_vector_dependency_v2");
    VectorDependencyV1 vector_attach=(VectorDependencyV1)api->dlsym_fn(image,"birnam_vector_dependency_v1");
    if (vector_v2_attach || vector_attach) {
        fixture_prepare_vector(api);
        if(vector_v2_attach) assert(vector_v2_attach(fixture_vector_v2)==1);
        else assert(vector_attach(fixture_vector_v1)==1);
    }
    RangeDependencyV2 range_v2_attach=(RangeDependencyV2)api->dlsym_fn(image,"birnam_range_dependency_v2");
    RangeDependencyV1 range_attach=(RangeDependencyV1)api->dlsym_fn(image,"birnam_range_dependency_v1");
    /* Previous build support is confined to the independent baseline oracle. */
    RangeMethodsDependencyV1 previous_attach=(RangeMethodsDependencyV1)api->dlsym_fn(image,"birnam_range_methods_dependency_v1");
    if (range_v2_attach || range_attach || previous_attach) {
        if (!fixture_range_v2 && !fixture_range && !fixture_range_methods) {
            const char *slash=strrchr(fixture_floor_path,'/'); assert(slash);
            char path[PATH_MAX];
            int n=snprintf(path,sizeof(path),"%.*s/libbirnam_range.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
            assert(n>0 && (size_t)n<sizeof(path));
            void *range=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(range);
            fixture_attach_integer(api,range);
            ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(range,"module_init"); assert(init); init(api,NULL);
            if (range_v2_attach) {
                RangeProviderV2 provide=(RangeProviderV2)api->dlsym_fn(range,"birnam_range_provider_v2"); assert(provide);
                fixture_range_v2=provide(); assert(fixture_range_v2);
            } else if (range_attach) {
                RangeProviderV1 provide=(RangeProviderV1)api->dlsym_fn(range,"birnam_range_provider_v1"); assert(provide);
                fixture_range=provide(); assert(fixture_range);
            } else {
                RangeMethodsProviderV1 provide=(RangeMethodsProviderV1)api->dlsym_fn(range,"birnam_range_methods_provider_v1"); assert(provide);
                fixture_range_methods=provide(); assert(fixture_range_methods);
            }
        }
        if (range_v2_attach) assert(range_v2_attach(fixture_range_v2)==1);
        else if (range_attach) assert(range_attach(fixture_range)==1);
        else assert(previous_attach(fixture_range_methods)==1);
    }
    MapDependencyV2 map_v2_attach=(MapDependencyV2)api->dlsym_fn(image,"birnam_map_dependency_v2");
    MapDependencyV1 map_attach=(MapDependencyV1)api->dlsym_fn(image,"birnam_map_dependency_v1");
    if(map_v2_attach || map_attach) {
        fixture_prepare_map(api);
        if(map_v2_attach) assert(map_v2_attach(fixture_map_v2)==1);
        else assert(map_attach(fixture_map)==1);
    }
    StringCollectionDependencyV1 split_attach=(StringCollectionDependencyV1)api->dlsym_fn(image,"birnam_string_collection_dependency_v1");
    if(split_attach) { fixture_prepare_string_collection(api); assert(split_attach(fixture_string_collection)==1); }

}
static inline void fixture_prepare_vector(IdApi *api) {
    if(fixture_vector) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_vector.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *vector=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(vector);
    fixture_attach_integer(api,vector);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(vector,"module_init"); assert(init);
    VectorProviderV2 current=(VectorProviderV2)api->dlsym_fn(vector,"birnam_vector_provider_v2");
    if(current) {
        assert(!current()); init(api,NULL); fixture_vector_v2=current(); assert(fixture_vector_v2);
        fixture_vector_ops=(FixtureVectorOps){fixture_vector_v2->empty,fixture_vector_v2->push,fixture_vector_count,fixture_vector_at,fixture_vector_v2->behavior};
    } else {
        VectorProviderV1 previous=(VectorProviderV1)api->dlsym_fn(vector,"birnam_vector_provider_v1"); assert(previous && !previous());
        init(api,NULL); fixture_vector_v1=previous(); assert(fixture_vector_v1);
        fixture_vector_ops=(FixtureVectorOps){fixture_vector_v1->empty,fixture_vector_v1->push,fixture_vector_v1->count,fixture_vector_v1->at,fixture_vector_v1->behavior};
    }
    fixture_vector=&fixture_vector_ops;
}
static inline void fixture_prepare_map(IdApi *api) {
    if(fixture_map || fixture_map_v2) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_map.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *map=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(map); fixture_attach_integer(api,map);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(map,"module_init"); assert(init);
    MapProviderV2 current=(MapProviderV2)api->dlsym_fn(map,"birnam_map_provider_v2");
    if(current) {
        assert(!current()); init(api,NULL); fixture_map_v2=current(); assert(fixture_map_v2);
    } else {
        MapProviderV1 previous=(MapProviderV1)api->dlsym_fn(map,"birnam_map_provider_v1"); assert(previous && !previous());
        init(api,NULL); fixture_map=previous(); assert(fixture_map);
    }
}
static inline void fixture_prepare_string_collection(IdApi *api) {
    if(fixture_string_collection) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_string_collection.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    StringCollectionProviderV1 provide=(StringCollectionProviderV1)api->dlsym_fn(image,"birnam_string_collection_provider_v1"); assert(provide && !provide());
    init(api,NULL); fixture_string_collection=provide(); assert(fixture_string_collection);
}
static inline void fixture_mount_integer(IdApi *api, const char *floor_path) {
    assert(strlen(floor_path)<sizeof(fixture_floor_path));
    strcpy(fixture_floor_path,floor_path);
    fixture_prepare_string(api, floor_path);
    assert(!fixture_integer);
    fixture_prepare_word(api, floor_path);
    fixture_prepare_float(api, floor_path);
    const char *slash = strrchr(floor_path, '/'); assert(slash);
    char path[PATH_MAX];
    int length = snprintf(path, sizeof(path), "%.*s/libbirnam_integer.dylib", (int)(slash-floor_path), floor_path);
    assert(length > 0 && (size_t)length < sizeof(path));
    void *image = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!image) fprintf(stderr, "%s\n", dlerror());
    assert(image);
    ModuleInitFn init = (ModuleInitFn)api->dlsym_fn(image, "module_init"); assert(init);
    fixture_attach_float(api, image);
    fixture_attach_string(api, image);
    init(api, NULL);
    IntegerProviderV1 provide = (IntegerProviderV1)api->dlsym_fn(image, "birnam_integer_provider_v1"); assert(provide);
    fixture_integer = provide(); assert(fixture_integer);
    assert(fixture_integer->version == 1 && fixture_integer->byte_size == sizeof(IntegerApiV1));
    void *floor = api->dlopen_fn(floor_path, RTLD_NOW | RTLD_LOCAL); assert(floor);
    fixture_attach_integer(api, floor);
    fixture_attach_integer(api, fixture_float_image);
    fixture_attach_integer(api, fixture_word_image);
}
#endif
