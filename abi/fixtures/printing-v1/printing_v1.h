/* Independent Object/Symbol printing installation descriptor; include IdApi first. */
#ifndef BIRNAM_PRINTING_V1_H
#define BIRNAM_PRINTING_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(IdApi *registration,const IdApi *execution);
} PrintingApiV1;
typedef const PrintingApiV1 *(*PrintingProviderV1)(void);
typedef int (*PrintingDependencyV1)(const PrintingApiV1 *);
_Static_assert(sizeof(PrintingApiV1)==24 && offsetof(PrintingApiV1,install)==16,"String collection boundary shape");
#endif
