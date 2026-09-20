#ifndef FIXTURE_INTEGER_SUPPORT_H
#define FIXTURE_INTEGER_SUPPORT_H
#include "namespace-support.h"
#include "../../abi/fixtures/integer-v1/integer_v1.h"
#include "float-support.h"
#include "word-support.h"
#include "string-support.h"
#include "../../abi/fixtures/range-methods-v1/range_methods_v1.h"
#include "../../abi/fixtures/range-v1/range_v1.h"
#include "../../abi/fixtures/range-v6/range_v6.h"
#include "../../abi/fixtures/vector-v6/vector_v6.h"
#include "../../abi/fixtures/map-v6/map_v6.h"
#include "../../abi/fixtures/string-collection-v1/string_collection_v1.h"
#include "../../abi/fixtures/printing-v1/printing_v1.h"
#include "../../abi/fixtures/system-gc-v2/system_gc_v2.h"
#include "../../abi/fixtures/system-output-v1/system_output_v1.h"
#include "../../abi/fixtures/system-input-v3/system_input_v3.h"
#include "../../abi/fixtures/environment-v3/environment_v3.h"
#include "../../abi/fixtures/native-loader-v12/native_loader_v12.h"
#include "../../abi/fixtures/source-loader-v3/source_loader_v3.h"
#include "../../abi/fixtures/file-io-v3/file_io_v3.h"
#include "../../abi/fixtures/block-loop-v3/block_loop_v3.h"
#include "../../abi/fixtures/text-view-v1/text_view_v1.h"
#include "../../abi/fixtures/error-value-v3/error_value_v3.h"
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
static const RangeApiV6 *fixture_range_v2;
static const VectorApiV1 *fixture_vector_v1; /* Historical storage oracle only. */
static const VectorApiV6 *fixture_vector_v2;
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
static inline void fixture_prepare_vector(IdApi *api,const RootRegistryV1 *root_registry);
static const CollectionValueApiV1 *fixture_collection_value;
static const MapApiV1 *fixture_map; /* Historical storage oracle only. */
static const MapApiV6 *fixture_map_v2;
static inline void fixture_prepare_map(IdApi *api,const RootRegistryV1 *root_registry);
static const ErrorValueApiV3 *fixture_error_value;
static inline void fixture_prepare_error_value(IdApi *api,const RootRegistryV1 *root_registry);
static const TextViewApiV1 *fixture_text_view;
static inline void fixture_prepare_text_view(IdApi *api,const RootRegistryV1 *root_registry);
static const BlockLoopApiV3 *fixture_block_loop;
static inline void fixture_prepare_block_loop(IdApi *api,const RootRegistryV1 *root_registry);
static const FileIoApiV3 *fixture_file_io;
static inline void fixture_prepare_file_io(IdApi *api,const RootRegistryV1 *root_registry);
static const SourceLoaderApiV3 *fixture_source_loader;
static inline void fixture_prepare_source_loader(IdApi *api,const RootRegistryV1 *root_registry);
static const NativeLoaderApiV12 *fixture_native_loader;
static inline void fixture_prepare_native_loader(IdApi *api,const RootRegistryV1 *root_registry);
static const EnvironmentApiV3 *fixture_environment;
static inline void fixture_prepare_environment(IdApi *api,const RootRegistryV1 *root_registry);
static const SystemInputApiV3 *fixture_system_input;
static inline void fixture_prepare_system_input(IdApi *api,const RootRegistryV1 *root_registry);
static const SystemOutputApiV1 *fixture_system_output;
static inline void fixture_prepare_system_output(IdApi *api,const RootRegistryV1 *root_registry);
static const SystemGcApiV2 *fixture_system_gc;
static inline void fixture_prepare_system_gc(IdApi *api,const RootRegistryV1 *root_registry);
static const PrintingApiV1 *fixture_printing;
static inline void fixture_prepare_printing(IdApi *api,const RootRegistryV1 *root_registry);
static const StringCollectionApiV1 *fixture_string_collection;
static inline void fixture_prepare_string_collection(IdApi *api,const RootRegistryV1 *root_registry);
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
static inline void fixture_attach_integer(IdApi *api, void *image,const RootRegistryV1 *root_registry) {
    fixture_attach_namespace_names(api,image);
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
            void *value=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(value); fixture_attach_integer(api,value,root_registry);
            ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(value,"module_init"); assert(init);
            CollectionValueProviderV1 provide=(CollectionValueProviderV1)api->dlsym_fn(value,"birnam_collection_value_provider_v1"); assert(provide);
            assert(!provide()); init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_collection_value=provide(); assert(fixture_collection_value);
        }
        assert(value_attach(fixture_collection_value)==1);
    }
    VectorDependencyV6 vector_v2_attach=(VectorDependencyV6)api->dlsym_fn(image,"birnam_vector_dependency_v6");
    VectorDependencyV1 vector_attach=(VectorDependencyV1)api->dlsym_fn(image,"birnam_vector_dependency_v1");
    if (vector_v2_attach || vector_attach) {
        fixture_prepare_vector(api,root_registry);
        if(vector_v2_attach) assert(vector_v2_attach(fixture_vector_v2)==1);
        else assert(vector_attach(fixture_vector_v1)==1);
    }
    RangeDependencyV6 range_v2_attach=(RangeDependencyV6)api->dlsym_fn(image,"birnam_range_dependency_v6");
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
            fixture_attach_integer(api,range,root_registry);
            ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(range,"module_init"); assert(init); init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());
            if (range_v2_attach) {
                RangeProviderV6 provide=(RangeProviderV6)api->dlsym_fn(range,"birnam_range_provider_v6"); assert(provide);
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
    MapDependencyV6 map_v2_attach=(MapDependencyV6)api->dlsym_fn(image,"birnam_map_dependency_v6");
    MapDependencyV1 map_attach=(MapDependencyV1)api->dlsym_fn(image,"birnam_map_dependency_v1");
    if(map_v2_attach || map_attach) {
        fixture_prepare_map(api,root_registry);
        if(map_v2_attach) assert(map_v2_attach(fixture_map_v2)==1);
        else assert(map_attach(fixture_map)==1);
    }
    ErrorValueDependencyV3 error_attach=(ErrorValueDependencyV3)api->dlsym_fn(image,"birnam_error_value_dependency_v3");
    if(error_attach) { fixture_prepare_error_value(api,root_registry); assert(error_attach(fixture_error_value)==1); }
    TextViewDependencyV1 text_attach=(TextViewDependencyV1)api->dlsym_fn(image,"birnam_text_view_dependency_v1");
    if(text_attach) { fixture_prepare_text_view(api,root_registry); assert(text_attach(fixture_text_view)==1); }
    SystemOutputDependencyV1 output_attach=(SystemOutputDependencyV1)api->dlsym_fn(image,"birnam_system_output_dependency_v1");
    if(output_attach) { fixture_prepare_system_output(api,root_registry); assert(output_attach(fixture_system_output)==1); }
    SystemInputDependencyV3 input_attach=(SystemInputDependencyV3)api->dlsym_fn(image,"birnam_system_input_dependency_v3");
    if(input_attach) { fixture_prepare_system_input(api,root_registry); assert(input_attach(fixture_system_input)==1); }
    EnvironmentDependencyV3 environment_attach=(EnvironmentDependencyV3)api->dlsym_fn(image,"birnam_environment_dependency_v3");
    if(environment_attach) { fixture_prepare_environment(api,root_registry); assert(environment_attach(fixture_environment)==1); }
    NativeLoaderDependencyV12 native_loader_attach=(NativeLoaderDependencyV12)api->dlsym_fn(image,"birnam_native_loader_dependency_v12");
    if(native_loader_attach) { fixture_prepare_native_loader(api,root_registry); assert(native_loader_attach(fixture_native_loader)==1); }
    SourceLoaderDependencyV3 source_loader_attach=(SourceLoaderDependencyV3)api->dlsym_fn(image,"birnam_source_loader_dependency_v3");
    if(source_loader_attach) { fixture_prepare_source_loader(api,root_registry); assert(source_loader_attach(fixture_source_loader)==1); }
    FileIoDependencyV3 file_io_attach=(FileIoDependencyV3)api->dlsym_fn(image,"birnam_file_io_dependency_v3");
    if(file_io_attach) { fixture_prepare_file_io(api,root_registry); assert(file_io_attach(fixture_file_io)==1); }
    BlockLoopDependencyV3 block_loop_attach=(BlockLoopDependencyV3)api->dlsym_fn(image,"birnam_block_loop_dependency_v3");
    if(block_loop_attach) { fixture_prepare_block_loop(api,root_registry); assert(block_loop_attach(fixture_block_loop)==1); }
    SystemGcDependencyV2 gc_attach=(SystemGcDependencyV2)api->dlsym_fn(image,"birnam_system_gc_dependency_v2");
    if(gc_attach) { fixture_prepare_system_gc(api,root_registry); assert(gc_attach(fixture_system_gc)==1); }
    PrintingDependencyV1 printing_attach=(PrintingDependencyV1)api->dlsym_fn(image,"birnam_printing_dependency_v1");
    if(printing_attach) { fixture_prepare_printing(api,root_registry); assert(printing_attach(fixture_printing)==1); }
    StringCollectionDependencyV1 split_attach=(StringCollectionDependencyV1)api->dlsym_fn(image,"birnam_string_collection_dependency_v1");
    if(split_attach) { fixture_prepare_string_collection(api,root_registry); assert(split_attach(fixture_string_collection)==1); }

}
static inline void fixture_prepare_vector(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_vector) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_vector.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *vector=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(vector);
    fixture_attach_integer(api,vector,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(vector,"module_init"); assert(init);
    VectorProviderV6 current=(VectorProviderV6)api->dlsym_fn(vector,"birnam_vector_provider_v6");
    if(current) {
        assert(!current()); init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_vector_v2=current(); assert(fixture_vector_v2);
        fixture_vector_ops=(FixtureVectorOps){fixture_vector_v2->empty,fixture_vector_v2->push,fixture_vector_count,fixture_vector_at,fixture_vector_v2->behavior};
    } else {
        VectorProviderV1 previous=(VectorProviderV1)api->dlsym_fn(vector,"birnam_vector_provider_v1"); assert(previous && !previous());
        init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_vector_v1=previous(); assert(fixture_vector_v1);
        fixture_vector_ops=(FixtureVectorOps){fixture_vector_v1->empty,fixture_vector_v1->push,fixture_vector_v1->count,fixture_vector_v1->at,fixture_vector_v1->behavior};
    }
    fixture_vector=&fixture_vector_ops;
}
static inline void fixture_prepare_map(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_map || fixture_map_v2) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_map.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *map=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(map); fixture_attach_integer(api,map,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(map,"module_init"); assert(init);
    MapProviderV6 current=(MapProviderV6)api->dlsym_fn(map,"birnam_map_provider_v6");
    if(current) {
        assert(!current()); init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_map_v2=current(); assert(fixture_map_v2);
    } else {
        MapProviderV1 previous=(MapProviderV1)api->dlsym_fn(map,"birnam_map_provider_v1"); assert(previous && !previous());
        init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_map=previous(); assert(fixture_map);
    }
}
static inline void fixture_prepare_error_value(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_error_value) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_error_value.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    ErrorValueProviderV3 provide=(ErrorValueProviderV3)api->dlsym_fn(image,"birnam_error_value_provider_v3"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_error_value=provide(); assert(fixture_error_value);
}
static inline void fixture_prepare_text_view(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_text_view) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_text_view.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    TextViewProviderV1 provide=(TextViewProviderV1)api->dlsym_fn(image,"birnam_text_view_provider_v1"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_text_view=provide(); assert(fixture_text_view);
}
static inline void fixture_prepare_system_output(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_system_output) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_system_output.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    SystemOutputProviderV1 provide=(SystemOutputProviderV1)api->dlsym_fn(image,"birnam_system_output_provider_v1"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_system_output=provide(); assert(fixture_system_output);
}
static inline void fixture_prepare_system_input(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_system_input) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_system_input.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    SystemInputProviderV3 provide=(SystemInputProviderV3)api->dlsym_fn(image,"birnam_system_input_provider_v3"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_system_input=provide(); assert(fixture_system_input);
}
static inline void fixture_prepare_environment(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_environment) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_environment.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    EnvironmentProviderV3 provide=(EnvironmentProviderV3)api->dlsym_fn(image,"birnam_environment_provider_v3"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_environment=provide(); assert(fixture_environment);
}
static inline void fixture_prepare_native_loader(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_native_loader) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_native_loader.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    NativeLoaderProviderV12 provide=(NativeLoaderProviderV12)api->dlsym_fn(image,"birnam_native_loader_provider_v12"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_native_loader=provide(); assert(fixture_native_loader);
}
static inline void fixture_prepare_source_loader(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_source_loader) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_source_loader.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    SourceLoaderProviderV3 provide=(SourceLoaderProviderV3)api->dlsym_fn(image,"birnam_source_loader_provider_v3"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_source_loader=provide(); assert(fixture_source_loader);
}
static inline void fixture_prepare_file_io(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_file_io) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_file_io.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    FileIoProviderV3 provide=(FileIoProviderV3)api->dlsym_fn(image,"birnam_file_io_provider_v3"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_file_io=provide(); assert(fixture_file_io);
}
static inline void fixture_prepare_block_loop(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_block_loop) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_block_loop.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    BlockLoopProviderV3 provide=(BlockLoopProviderV3)api->dlsym_fn(image,"birnam_block_loop_provider_v3"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_block_loop=provide(); assert(fixture_block_loop);
}
static inline void fixture_prepare_system_gc(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_system_gc) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_system_gc.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    SystemGcProviderV2 provide=(SystemGcProviderV2)api->dlsym_fn(image,"birnam_system_gc_provider_v2"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_system_gc=provide(); assert(fixture_system_gc);
}
static inline void fixture_prepare_printing(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_printing) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_printing.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    PrintingProviderV1 provide=(PrintingProviderV1)api->dlsym_fn(image,"birnam_printing_provider_v1"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_printing=provide(); assert(fixture_printing);
}
static inline void fixture_prepare_string_collection(IdApi *api,const RootRegistryV1 *root_registry) {
    if(fixture_string_collection) return;
    const char *slash=strrchr(fixture_floor_path,'/'); assert(slash); char path[PATH_MAX];
    int n=snprintf(path,sizeof(path),"%.*s/libbirnam_string_collection.dylib",(int)(slash-fixture_floor_path),fixture_floor_path);
    assert(n>0 && (size_t)n<sizeof(path));
    void *image=api->dlopen_fn(path,RTLD_NOW|RTLD_LOCAL); assert(image); fixture_attach_integer(api,image,root_registry);
    ModuleInitFn init=(ModuleInitFn)api->dlsym_fn(image,"module_init"); assert(init);
    StringCollectionProviderV1 provide=(StringCollectionProviderV1)api->dlsym_fn(image,"birnam_string_collection_provider_v1"); assert(provide && !provide());
    init(api,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers()); fixture_string_collection=provide(); assert(fixture_string_collection);
}
static inline void fixture_mount_integer(IdApi *api, const char *floor_path, const NamespacePresenceV1 *presence, const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const RootRegistryV1 *root_registry) {
    assert(strlen(floor_path)<sizeof(fixture_floor_path));
    strcpy(fixture_floor_path,floor_path);
    fixture_prepare_string(api, floor_path,exports,root_registry);
    assert(!fixture_integer);
    fixture_prepare_word(api, floor_path, presence, imports,exports,root_registry);
    fixture_prepare_float(api, floor_path, presence, imports,exports,root_registry);
    const char *slash = strrchr(floor_path, '/'); assert(slash);
    char path[PATH_MAX];
    int length = snprintf(path, sizeof(path), "%.*s/libbirnam_integer.dylib", (int)(slash-floor_path), floor_path);
    assert(length > 0 && (size_t)length < sizeof(path));
    void *image = api->dlopen_fn(path, RTLD_NOW | RTLD_LOCAL);
    if (!image) fprintf(stderr, "%s\n", dlerror());
    assert(image);
    ModuleInitFn init = (ModuleInitFn)api->dlsym_fn(image, "module_init"); assert(init);
    fixture_attach_integer(api, image,root_registry);
    init(api,NULL,presence,imports,exports,fixture_vtable_enumeration(),fixture_method_observation(),root_registry,fixture_gc_settings(),fixture_payload_layout(),fixture_trace_hooks(),fixture_finalizers());
    IntegerProviderV1 provide = (IntegerProviderV1)api->dlsym_fn(image, "birnam_integer_provider_v1"); assert(provide);
    fixture_integer = provide(); assert(fixture_integer);
    assert(fixture_integer->version == 1 && fixture_integer->byte_size == sizeof(IntegerApiV1));
    void *floor = api->dlopen_fn(floor_path, RTLD_NOW | RTLD_LOCAL); assert(floor);
    fixture_attach_integer(api, floor,root_registry);
    fixture_attach_integer(api, fixture_float_image,root_registry);
    fixture_attach_integer(api, fixture_word_image,root_registry);
}
#endif
