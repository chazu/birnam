/* Real native declaration fixture. Only the successful Integer subset is
 * declared; failure paths deliberately count their ordinary native entry. */
#include <stddef.h>
#include <stdint.h>
#include <limits.h>
typedef void *Oop;
typedef Oop (*Method)(Oop, const Oop *, size_t);
static Oop (*make_integer)(int64_t);
static int64_t (*read_integer)(Oop);
static int (*is_error)(Oop);
static Oop nil;
static Oop integer_vtable;
static unsigned long calls;
static unsigned declarations_enabled = 1;
void numeric_setup(void *make, void *read, void *error, Oop nil_value, Oop integer_vtable_value) {
    make_integer = make; read_integer = read; is_error = error; nil = nil_value;
    integer_vtable = integer_vtable_value;
}
unsigned long numeric_calls(void) { return calls; }
void numeric_declarations_enabled(unsigned enabled) { declarations_enabled = enabled; }
static Oop evaluate(unsigned op, Oop receiver, const Oop *args, size_t count) {
    ++calls;
    if (count != 1) return nil;
    int64_t a = read_integer(receiver);
    if (is_error(args[0])) return args[0];
    int64_t b = read_integer(args[0]);
    if ((op == 4 || op == 5) && b == 0) return nil;
    __int128 result;
    switch (op) {
        case 1: result = (__int128)a + b; break;
        case 2: result = (__int128)a - b; break;
        case 3: result = (__int128)a * b; break;
        case 4: result = (__int128)a / b; break;
        default: result = (__int128)a % b; break;
    }
    if (result < INT64_MIN || result > INT64_MAX) return nil;
    return make_integer((int64_t)result);
}
#define OP(N) Oop numeric_##N(Oop r,const Oop *a,size_t n){return evaluate(N,r,a,n);}
OP(1) OP(2) OP(3) OP(4) OP(5)
Oop numeric_unknown(Oop r,const Oop *a,size_t n){return evaluate(2,r,a,n);}
unsigned int birnam_native_numeric_semantics_v3(const void *method, Oop requested_vtable, const uintptr_t *functions) {
    if (!functions || !declarations_enabled) return 0;
    if (requested_vtable != integer_vtable) return 0;
    Method methods[] = { numeric_1, numeric_2, numeric_3, numeric_4, numeric_5 };
    for (unsigned i = 0; i < 5; ++i) if (method == (void *)methods[i]) return i + 1;
    return 0;
}
