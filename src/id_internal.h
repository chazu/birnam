#ifndef __ID_INTERNAL_H
#define __ID_INTERNAL_H

#include "id.h"

// Internal struct definitions for libid objects
// These are shared between libid.c and code that needs to introspect objects

struct t__selector
{
  size_t	 size;
  char	 	*elements;
};

struct t__assoc
{
  oop		 key;
  oop		 value;
};

struct t__closure
{
  oop		 method;
  oop		 data;
};

struct t__vector
{
  size_t	 size;
  oop		 elements[0];	/* INLINE */
};

struct t__vtable
{
  size_t	 tally;
  oop		 bindings;
  oop		 delegate;
};

struct t__object
{
  union {
    oop		_vtable[0];
    struct t__closure	 closure;
    struct t__selector	 selector;
    struct t__assoc	 assoc;
    struct t__vector	 vector;
    struct t__vtable	 vtable;
  };
};

// Type aliases for convenience
typedef struct t__object   _object_t;
typedef struct t__selector _selector_t;
typedef struct t__closure  _closure_t;
typedef struct t__assoc    _assoc_t;
typedef struct t__vector   _vector_t;
typedef struct t__vtable   _vtable_t;

#endif /* __ID_INTERNAL_H */
