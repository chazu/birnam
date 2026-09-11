/* 74G test-local transport of existing generated-code shim signatures.
 * No IdApi field or runtime export is introduced by this fixture. */
typedef void* (*Method)(void*, void* const*, unsigned long);
typedef struct {
    void* (*send)(void*, void*, void**, long);
    void* (*intern)(const char*);
    void* (*integer)(long);
    long (*value)(void*);
    void* (*nil)(void);
    void* (*global)(const char*);
} FixtureApi;
static FixtureApi api;
void fixture_init(const FixtureApi* supplied) { api = *supplied; }
static void* birnam_send(void* r, void* s, void** a, long n) { return api.send(r,s,a,n); }
static void* birnam_intern(const char* s) { return api.intern(s); }
static void* birnam_mkSmallInt(long n) { return api.integer(n); }
static long birnam_smallIntValue(void* o) { return api.value(o); }
static void* birnam_nil(void) { return api.nil(); }
static void* birnam_global(const char* s) { return api.global(s); }
#define MKV(n) birnam_mkSmallInt(n)
