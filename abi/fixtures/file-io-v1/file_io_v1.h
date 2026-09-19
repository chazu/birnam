/* Independent File IO installation descriptor; include IdApi first. */
#ifndef BIRNAM_FILE_IO_V1_H
#define BIRNAM_FILE_IO_V1_H
typedef struct {
    uint32_t version; size_t byte_size;
    void (*install)(const IdApi *registration,const IdApi *execution,Oop behavior,Oop read_file,Oop write_file);
} FileIoApiV1;
typedef const FileIoApiV1 *(*FileIoProviderV1)(void);
typedef int (*FileIoDependencyV1)(const FileIoApiV1 *);
_Static_assert(sizeof(FileIoApiV1)==24 && offsetof(FileIoApiV1,install)==16,"File IO descriptor layout");
#endif
