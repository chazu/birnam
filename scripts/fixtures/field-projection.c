#include <stddef.h>
#include <stdint.h>
static void *floor_nil;
static unsigned int enabled = 1;
void field_setup(void *nil) { floor_nil = nil; }
void field_enable(unsigned int value) { enabled = value; }
void *field_project(void *receiver, void *const *arguments, size_t count) {
    (void)arguments;
    return receiver && count == 0 ? *(void **)receiver : floor_nil;
}
unsigned int birnam_native_field_projection_v1(const void *method, void *nil) {
    return enabled && nil == floor_nil && method == (const void *)&field_project;
}
