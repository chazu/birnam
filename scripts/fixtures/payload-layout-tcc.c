/* Independent old/new oracle for incoming family versus saved Unit selection. */
#ifdef PAYLOAD_LAYOUT_BASELINE
#include "idapi_v16.h"
#else
#include "idapi_v17.h"
#endif
#include "namespace-support.h"
#include "object-model-support.h"

static IdApi *api;
static unsigned first_count,saved_count,incoming_count;
static Oop first_types[2],saved_type,incoming_type;
static void apply(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    assert(!kinds && !count && !rest);
#ifdef PAYLOAD_LAYOUT_BASELINE
    api->set_layout(type,kinds,count,rest);
#else
    fixture_set_layout(type,kinds,count,rest);
#endif
}
static void first(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    assert(first_count<2);first_types[first_count++]=type;apply(type,kinds,count,rest);
}
static void saved(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    saved_count++;saved_type=type;apply(type,kinds,count,rest);
}
static void incoming(Oop type,const uint8_t *kinds,size_t count,uint8_t rest) {
    incoming_count++;incoming_type=type;apply(type,kinds,count,rest);
}
int main(int argc,char **argv) {
    assert(argc==3);
    void *floor=dlopen(argv[1],RTLD_NOW|RTLD_LOCAL);assert(floor);
    fixture_prepare_namespace(floor,argv[1]);api=((IdInitFn)dlsym(floor,"id_init"))();assert(api);
    fixture_set_auto(0);fixture_mount_object_model(api,argv[1],fixture_root_registry());
    void *image=dlopen(argv[2],RTLD_NOW|RTLD_LOCAL);assert(image);
    fixture_attach_integer(api,image,fixture_root_registry());
    ModuleInitFn initialize=(ModuleInitFn)dlsym(image,"module_init");assert(initialize);
    IdApi selected=*api,other=*api;
#ifdef PAYLOAD_LAYOUT_BASELINE
    selected.set_layout=first;other.set_layout=incoming;
#define MOUNT(table,cap) initialize(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings())
#else
    FixtureLayoutSlots selected_slots={first,fixture_slot_kind},other_slots={incoming,fixture_slot_kind};
    PayloadLayoutV1 selected_layouts=FIXTURE_LAYOUTS(selected_slots),other_layouts=FIXTURE_LAYOUTS(other_slots);
#define MOUNT(table,cap) initialize(table,NULL,fixture_namespace_presence(),fixture_namespace_imports(),fixture_namespace_exports(),fixture_vtable_enumeration(),fixture_method_observation(),fixture_root_registry(),fixture_gc_settings(),cap)
#endif
    MOUNT(&selected,&selected_layouts);
    Oop foreign=fixture_namespace_import("ForeignAddressVt"),unit=fixture_namespace_import("TccUnitVt");
    assert(foreign && unit && first_count==2 && first_types[0]==foreign && first_types[1]==unit);
#ifdef PAYLOAD_LAYOUT_BASELINE
    selected.set_layout=saved;
#else
    selected_slots.set=saved;
#endif
    MOUNT(&other,&other_layouts);
    assert(saved_count==1 && saved_type==unit && !incoming_count);
    fixture_namespace_export("ForeignAddress",NULL);fixture_namespace_export("ForeignAddressVt",NULL);
    MOUNT(&other,&other_layouts);
    assert(saved_count==2 && saved_type==unit && incoming_count==1);
    assert(incoming_type==fixture_namespace_import("ForeignAddressVt") && incoming_type!=foreign);
    puts("payload-layout-tcc: incoming ForeignAddress construction and live saved Unit layout survive repeated initialization");
}
