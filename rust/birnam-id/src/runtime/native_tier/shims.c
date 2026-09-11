/* Internal compiler declarations bound to process-lifetime Runtime functions.
 * These are private symbols in an owned memory unit, not new floor exports. */
extern void* birnam_send(void*, void*, void**, long);
extern void* birnam_intern(const char*);
extern void* birnam_mkSmallInt(long);
extern void* birnam_nil(void);
extern void* birnam_true(void);
extern void* birnam_false(void);
#define MKV(n) birnam_mkSmallInt(n)
