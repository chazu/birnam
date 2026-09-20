/* 74J benchmark setup: ordinary native custom lookup forces dispatch fallback. */
#include "idapi_v1.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static Method original_lookup;

static Oop forwarding_lookup(Oop receiver, const Oop *args, size_t count) {
    return original_lookup(receiver, args, count);
}

unsigned int birnam_symbol_borrow_v1(void) { return 1; }

void *module_init(IdApi *api, void *prior,const NamespacePresenceV1 *presence,const PayloadLayoutV1 *layouts) { (void)layouts;
    (void)presence;
    (void)prior;
    Oop lookup = api->intern("lookup");
    original_lookup = (Method)api->send(api->vtable_vt, lookup, &lookup, 1);
    const char *mode = getenv("BIRNAM_74J_MODE");
    if (original_lookup == NULL || mode == NULL) {
        fputs("74J lookup setup failed\n", stderr);
        abort();
    }
    if (strcmp(mode, "fallback") == 0) {
        api->method(api->vtable_vt, lookup, forwarding_lookup);
        /* Prove the override is installed, outside all measured intervals. */
        if (api->send(api->vtable_vt, lookup, &lookup, 1) != (Oop)forwarding_lookup) {
            fputs("74J lookup override was not installed\n", stderr);
            abort();
        }
    } else if (strcmp(mode, "canonical") != 0) {
        fputs("74J unknown mode\n", stderr);
        abort();
    }
    printf("74j-mode %s\n", mode);
    return NULL;
}
