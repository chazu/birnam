#include "idapi_v15.h"

typedef struct {
    IdApi *api;
    const NamespacePresenceV1 *presence;
    const NamespaceImportV1 *imports;
    const NamespaceExportV1 *exports;
    const VtableEnumerationV2 *vtables;
    const MethodObservationV1 *observation;
    const RootRegistryV1 *roots;
    unsigned calls;
} FixtureState;

static FixtureState state;

__attribute__((visibility("default")))
void *module_init(IdApi *api, void *prior_state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports,const VtableEnumerationV2 *vtables,const MethodObservationV1 *observation,const RootRegistryV1 *root_registry) { (void)root_registry; (void)observation; (void)vtables;
    if (!presence || !imports || !exports || !vtables || !observation || !root_registry) return NULL;
    if (prior_state == NULL) {
        state.api = api;
        state.presence = presence;
        state.imports = imports;
        state.exports = exports;
        state.vtables = vtables;
        state.observation = observation;
        state.roots = root_registry;
        state.calls = 1;
        return &state;
    }
    if (prior_state != &state || state.api != api || state.presence != presence || state.imports != imports || state.exports != exports || state.vtables != vtables || state.observation != observation || state.roots != root_registry) return NULL;
    state.calls += 1;
    return &state;
}

__attribute__((visibility("default")))
unsigned fixture_module_calls(void *opaque_state) {
    FixtureState *fixture = opaque_state;
    return fixture == &state ? fixture->calls : 0;
}
