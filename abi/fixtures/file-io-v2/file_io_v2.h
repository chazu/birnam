/* Independent File IO installation descriptor; include IdApi first. */
#ifndef BIRNAM_FILE_IO_V2_H
#define BIRNAM_FILE_IO_V2_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,const NamespacePresenceV1 *presence,Oop behavior,Oop read_file,Oop write_file);
} FileIoApiV2;
typedef const FileIoApiV2 *(*FileIoProviderV2)(void);
typedef int (*FileIoDependencyV2)(const FileIoApiV2 *);
_Static_assert(sizeof(FileIoApiV2)==24 && offsetof(FileIoApiV2,install)==16,"File IO descriptor layout");
#endif
