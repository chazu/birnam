#define _DARWIN_C_SOURCE
#include "idapi_v20.h"
#include <assert.h>
#include <dlfcn.h>
#include <string.h>
static IdApi *api;
static const PayloadLayoutV1 *layouts;
static Oop kept,holder,behavior;
static unsigned active,calls,registered;
static PayloadLayoutV1 saved_layouts;
static Oop image_behavior;
static unsigned image_calls,image_done;
static uint8_t image_later_slot(void *context,void *vt,size_t word) {
    assert(context==saved_layouts.context);
    if(vt==image_behavior) {
        assert(word==image_calls%2);image_calls++;
        if(image_calls==5) {
            /* The third pass is encoding, after admission and edge discovery. */
            const uint8_t changed[]={IDAPI_V20_SLOT_RAW,9};
            payload_layout_set_v1(&saved_layouts,vt,changed,2,IDAPI_V20_SLOT_RAW);
        }
    }
    uint8_t kind=payload_layout_slot_v1(&saved_layouts,vt,word);
    if(vt==image_behavior && image_calls==6) {
        assert(kind==9);
        const uint8_t restored[]={IDAPI_V20_SLOT_RAW,IDAPI_V20_SLOT_RAW};
        payload_layout_set_v1(&saved_layouts,vt,restored,2,IDAPI_V20_SLOT_RAW);
        *(PayloadLayoutV1 *)layouts=saved_layouts;image_done=1;
    }
    return kind;
}
static uint8_t image_first_slot(void *context,void *vt,size_t word) {
    assert(context==saved_layouts.context);
    if(vt==image_behavior) {
        assert(!image_calls && word==0);image_calls=1;
        ((PayloadLayoutV1 *)layouts)->slot_kind=image_later_slot;
    }
    return payload_layout_slot_v1(&saved_layouts,vt,word);
}
static Oop arm_image(Oop self,const Oop *args,size_t count) {
    (void)self;assert(args && count==1 && args[0] && !image_behavior);
    image_behavior=api->vtable_of(args[0]);
    assert(api->object_payload_size(args[0])==2*sizeof(uintptr_t));
    saved_layouts=*layouts;
    assert(payload_layout_slot_v1(&saved_layouts,image_behavior,0)==IDAPI_V20_SLOT_RAW);
    assert(payload_layout_slot_v1(&saved_layouts,image_behavior,1)==IDAPI_V20_SLOT_RAW);
    ((PayloadLayoutV1 *)layouts)->slot_kind=image_first_slot;
    return args[0];
}
static Oop image_checked(Oop self,const Oop *args,size_t count) {
    (void)self;(void)args;assert(!count && image_done && image_calls==6);
    assert(layouts->slot_kind==saved_layouts.slot_kind);
    assert(payload_layout_slot_v1(layouts,image_behavior,1)==IDAPI_V20_SLOT_RAW);
    return api->true_obj;
}
static void roots(MarkFn mark) {
    mark(kept);mark(holder);
    if(active) {
        calls++;
        const uint8_t kind=IDAPI_V20_SLOT_OOP;
        payload_layout_set_v1(layouts,behavior,&kind,1,IDAPI_V20_SLOT_RAW);
        assert(api->gc_collect()==0);
    }
}
static Oop exercise(Oop self,const Oop *args,size_t count) {
    (void)self;assert(args && count==1 && args[0]);kept=args[0];
    behavior=api->proto(api->object_vt);Oop child=api->proto(behavior);
    assert(payload_layout_slot_v1(layouts,behavior,0)==IDAPI_V20_SLOT_UNKNOWN);
    uint8_t prefix[]={IDAPI_V20_SLOT_OOP,9,IDAPI_V20_SLOT_CODE};
    payload_layout_set_v1(layouts,behavior,prefix,3,IDAPI_V20_SLOT_RAW);prefix[0]=0;
    assert(payload_layout_slot_v1(layouts,behavior,0)==IDAPI_V20_SLOT_OOP);
    assert(payload_layout_slot_v1(layouts,behavior,1)==9);
    assert(payload_layout_slot_v1(layouts,behavior,2)==IDAPI_V20_SLOT_CODE);
    assert(payload_layout_slot_v1(layouts,behavior,SIZE_MAX)==IDAPI_V20_SLOT_RAW);
    assert(payload_layout_slot_v1(layouts,child,0)==IDAPI_V20_SLOT_UNKNOWN);
    payload_layout_set_v1(layouts,behavior,NULL,0,IDAPI_V20_SLOT_RAW);
    holder=api->alloc(behavior,sizeof(Oop));*(Oop *)holder=kept;
    calls=0;active=1;(void)api->gc_collect();active=0;
    assert(calls==1 && payload_layout_slot_v1(layouts,behavior,0)==IDAPI_V20_SLOT_OOP);
    assert(payload_layout_slot_v1(layouts,behavior,1)==IDAPI_V20_SLOT_RAW);
    assert(api->vtable_of(holder)==behavior && *(Oop *)holder==kept && api->vtable_of(kept));
    Oop result=kept;kept=holder=behavior=NULL;return result;
}
void *module_init(IdApi *selected,void *prior,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *selected_layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizer_cap) { (void)finalizer_cap; (void)hooks;
    (void)presence;(void)exports;(void)vtables;(void)observation;(void)settings;
    assert(selected && !prior && imports && root_registry && selected_layouts);
    if(api)assert(api==selected && layouts==selected_layouts);
    else { api=selected;layouts=selected_layouts; }
    assert(layouts->version==1 && layouts->byte_size==sizeof(*layouts));
    Dl_info owner;assert(dladdr((void *)layouts->set,&owner) && strstr(owner.dli_fname,"libbirnam_payload_layout.dylib"));
    assert(dladdr((void *)layouts->slot_kind,&owner) && strstr(owner.dli_fname,"libbirnam_payload_layout.dylib"));
    if(!registered) { root_registry_register_v1(root_registry,roots);registered=1; }
    Oop system=namespace_import_v1(imports,"SystemVt");assert(system);
    api->method(system,api->intern("payloadLayoutExercise:"),exercise);
    api->method(system,api->intern("payloadLayoutArmImage:"),arm_image);
    api->method(system,api->intern("payloadLayoutImageChecked"),image_checked);return NULL;
}
int birnam_symbol_borrow_v1(void) { return 1; }
