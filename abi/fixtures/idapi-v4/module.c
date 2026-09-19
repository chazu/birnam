#include "idapi_v4.h"

typedef struct {
    IdApi *api;
    unsigned calls;
} FixtureState;

static FixtureState state;

__attribute__((visibility("default")))
void *module_init(IdApi *api, void *prior_state) {
    if (prior_state == NULL) {
        state.api = api;
        state.calls = 1;
        return &state;
    }
    if (prior_state != &state || state.api != api) return NULL;
    state.calls += 1;
    return &state;
}

__attribute__((visibility("default")))
unsigned fixture_module_calls(void *opaque_state) {
    FixtureState *fixture = opaque_state;
    return fixture == &state ? fixture->calls : 0;
}
