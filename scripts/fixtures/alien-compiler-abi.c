/* Independent C values and callbacks; this fixture has no Birnam declarations. */
typedef struct { double x, y; } Pair;
typedef struct { float values[4]; } Four;
typedef struct { float values[5]; } Five;
typedef struct { double x; Pair pair; } Nested;
typedef struct { long x; int values[3]; } Mixed;

long ac_long(long value) { return value + 3; }
int ac_int(int value) { return value - 3; }
unsigned long ac_unsigned(unsigned long value) { return value; }
double ac_double(double value) { return value + 0.5; }
float ac_float(float value) { return value + 0.5f; }
const char *ac_string(const char *value) { return value; }
void *ac_pointer(void *value) { return value; }
static long stored;
void ac_store(long value) { stored = value; }
long ac_stored(void) { return stored; }
Pair ac_pair(Pair value) { value.x += 1; value.y += 2; return value; }
Four ac_four(Four value) { value.values[3] += 1; return value; }
Five ac_five(Five value) { value.values[4] += 1; return value; }
Nested ac_nested(Nested value) { value.pair.y += 1; return value; }
Mixed ac_mixed(Mixed value) { value.x += 1; value.values[2] += 2; return value; }
long ac_call_long(long (*callback)(long), long value) { return callback(value); }
Pair ac_call_pair(Pair (*callback)(Pair), Pair value) { return callback(value); }
Nested ac_call_nested(Nested (*callback)(Nested), Nested value) { return callback(value); }
