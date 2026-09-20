#include "idapi_v19.h"

typedef struct {
    IdApi *api;
    const NamespacePresenceV1 *presence;
    const NamespaceImportV1 *imports;
    const NamespaceExportV1 *exports;
    const VtableEnumerationV2 *vtables;
    const MethodObservationV1 *observation;
    const RootRegistryV1 *roots;
    const GcSettingsV1 *settings;
    const PayloadLayoutV1 *layouts;
    const TraceHooksV1 *hooks;
    const FinalizersV1 *finalizers;
    unsigned calls;
} FixtureState;

static FixtureState state;

__attribute__((visibility("default")))
void *module_init(IdApi *api, void *prior_state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry,const GcSettingsV1 *settings,const PayloadLayoutV1 *layouts,const TraceHooksV1 *hooks,const FinalizersV1 *finalizer_cap) { (void)finalizer_cap; (void)hooks; (void)layouts; (void)settings; (void)root_registry; (void)observation; (void)vtables;
    if (!presence || !imports || !exports || !vtables || !observation || !root_registry || !settings || !layouts || !hooks || !finalizer_cap) return NULL;
    if (prior_state == NULL) {
        state.api = api;
        state.presence = presence;
        state.imports = imports;
        state.exports = exports;
        state.vtables = vtables;
        state.observation = observation;
        state.roots = root_registry;
        state.settings = settings;
        state.layouts = layouts;
        state.hooks = hooks;
        state.finalizers = finalizer_cap;
        state.calls = 1;
        return &state;
    }
    if (prior_state != &state || state.api != api || state.presence != presence || state.imports != imports || state.exports != exports || state.vtables != vtables || state.observation != observation || state.roots != root_registry || state.settings != settings || state.layouts != layouts || state.hooks != hooks || state.finalizers != finalizer_cap) return NULL;
    state.calls += 1;
    return &state;
}

__attribute__((visibility("default")))
unsigned fixture_module_calls(void *opaque_state) {
    FixtureState *fixture = opaque_state;
    return fixture == &state ? fixture->calls : 0;
}
