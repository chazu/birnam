/* Independent File IO installation descriptor; include IdApi first. */
#ifndef BIRNAM_FILE_IO_V3_H
#define BIRNAM_FILE_IO_V3_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,const NamespaceImportV1 *imports,Oop behavior,Oop read_file,Oop write_file);
} FileIoApiV3;
typedef const FileIoApiV3 *(*FileIoProviderV3)(void);
typedef int (*FileIoDependencyV3)(const FileIoApiV3 *);
_Static_assert(sizeof(FileIoApiV3)==24 && offsetof(FileIoApiV3,install)==16,"File IO descriptor layout");
#endif
