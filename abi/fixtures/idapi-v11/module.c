#include "idapi_v11.h"

typedef struct {
    IdApi *api;
    const NamespacePresenceV1 *presence;
    const NamespaceImportV1 *imports;
    const NamespaceExportV1 *exports;
    unsigned calls;
} FixtureState;

static FixtureState state;

__attribute__((visibility("default")))
void *module_init(IdApi *api, void *prior_state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,const NamespaceExportV1 *exports) {
    if (!presence || !imports || !exports) return NULL;
    if (prior_state == NULL) {
        state.api = api;
        state.presence = presence;
        state.imports = imports;
        state.exports = exports;
        state.calls = 1;
        return &state;
    }
    if (prior_state != &state || state.api != api || state.presence != presence || state.imports != imports || state.exports != exports) return NULL;
    state.calls += 1;
    return &state;
}

__attribute__((visibility("default")))
unsigned fixture_module_calls(void *opaque_state) {
    FixtureState *fixture = opaque_state;
    return fixture == &state ? fixture->calls : 0;
}
