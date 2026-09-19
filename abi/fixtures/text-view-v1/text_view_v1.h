/* Independent shared text-byte view mirror; include IdApi first. */
#ifndef BIRNAM_TEXT_VIEW_V1_H
#define BIRNAM_TEXT_VIEW_V1_H
typedef struct { const uint8_t *data; size_t length; } TextBytesV1;
typedef TextBytesV1 (*TextViewFnV1)(const IdApi *,Oop);
typedef struct { uint32_t version; size_t byte_size; TextViewFnV1 exact,printable; } TextViewApiV1;
typedef const TextViewApiV1 *(*TextViewProviderV1)(void);
typedef int (*TextViewDependencyV1)(const TextViewApiV1 *);
_Static_assert(sizeof(TextBytesV1)==16 && offsetof(TextBytesV1,length)==8,"text view result layout");
_Static_assert(sizeof(TextViewApiV1)==32 && offsetof(TextViewApiV1,printable)==24,"text view descriptor layout");
#endif
