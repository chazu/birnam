#include "idapi_v10.h"

typedef struct {
    IdApi *api;
    const NamespacePresenceV1 *presence;
    const NamespaceImportV1 *imports;
    unsigned calls;
} FixtureState;

static FixtureState state;

__attribute__((visibility("default")))
void *module_init(IdApi *api, void *prior_state,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports) {
    if (!presence || !imports) return NULL;
    if (prior_state == NULL) {
        state.api = api;
        state.presence = presence;
        state.imports = imports;
        state.calls = 1;
        return &state;
    }
    if (prior_state != &state || state.api != api || state.presence != presence || state.imports != imports) return NULL;
    state.calls += 1;
    return &state;
}

__attribute__((visibility("default")))
unsigned fixture_module_calls(void *opaque_state) {
    FixtureState *fixture = opaque_state;
    return fixture == &state ? fixture->calls : 0;
}
