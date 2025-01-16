#include <stdio.h>
#include <stdlib.h>
#include <string.h>

 static struct vtable;
 static struct object;
 static struct symbol;

// Define function pointer type for function objects
typedef struct object *(*oop)(struct object *receiver, ...);

struct vtable {
  struct vtable *_vt[0];
  int size;
  int tally;
  struct object **keys;
  struct object **values;
  struct vtable *parent;
};

struct object {
  struct vtable *_vt[0];
};

struct symbol {
  struct vtable *_vt[0];
  char *string;
};

// Define primitives for object system
static struct vtable *metavtable = 0;
static struct vtable *object_vt = 0;
static struct vtable *symbol_vt = 0;

static struct object *s_addMethod = 0;
static struct object *s_allocate = 0;
static struct object *s_delegated = 0;
static struct object *s_lookup = 0;
static struct object *s_intern = 0;

static struct object *symbol = 0;

static struct vtable *SymbolList = 0;

static inline void *alloc(size_t size) {
  struct vtable **ppvt =
      (struct vtable **)calloc(1, sizeof(struct vtable *) + size);
  return (void *)(ppvt + 1);
}

static struct object *symbol_new(char *string) {
  struct symbol *symbol = (struct symbol *)alloc(sizeof(struct symbol));
  symbol->_vt[-1] = symbol_vt;
  symbol->string = strdup(string);
  return (struct object *)symbol;
}

static struct object *vtable_lookup(struct vtable *self, struct object *key);

#if ICACHE
#define send(RCV, MSG, ARGS...)                                                \
  ({                                                                           \
    struct object *r = (struct object *)(RCV);                                 \
    struct vtable *thisVT = r->_vt[-1];                                        \
    static struct vtable *prevVT = 0;                                          \
    static oop method = 0;                                                \
    (thisVT == prevVT ? method : (prevVT = thisVT, method = _bind(r, (MSG))))( \
        r, ##ARGS);                                                            \
  })
#else
#define send(RCV, MSG, ARGS...)                                                \
  ({                                                                           \
    struct object *r = (struct object *)(RCV);                                 \
    oop method = _bind(r, (MSG));                                         \
    method(r, ##ARGS);                                                         \
  })
#endif

#if MCACHE
static struct entry {
  struct vtable *vtable;
  struct object *selector;
  oop method;
} MethodCache[8192];
#endif

static oop _bind(struct object *rcv, struct object *msg) {
  oop method;
  struct vtable *vt = rcv->_vt[-1];
#if MCACHE
  unsigned int hash = (((unsigned)vt << 2) ^ ((unsigned)msg >> 3)) &
                      ((sizeof(MethodCache) / sizeof(struct entry)) - 1);
  struct entry *line = MethodCache + hash;
  if (line->vtable == vt && line->selector == msg)
    return line->method;
#endif
  method = ((msg == s_lookup) && (rcv == (struct object *)metavtable))
               ? (oop)vtable_lookup(vt, msg)
               : (oop)send(vt, s_lookup, msg);
#if MCACHE
  line->vtable = vt;
  line->selector = msg;
  line->method = method;
#endif
  return method;
}

static struct object *vtable_allocate(struct vtable *self, int payloadSize) {
  struct object *object = (struct object *)alloc(payloadSize);
  object->_vt[-1] = self;
  return object;
}

static struct vtable *vtable_delegated(struct vtable *self) {
  struct vtable *child =
      (struct vtable *)vtable_allocate(self, sizeof(struct vtable));
  child->_vt[-1] = self ? self->_vt[-1] : 0;
  child->size = 2;
  child->tally = 0;
  child->keys = (struct object **)calloc(child->size, sizeof(struct object *));
  child->values =
      (struct object **)calloc(child->size, sizeof(struct object *));
  child->parent = self;
  return child;
}
static struct object *vtable_addMethod(struct vtable *self, struct object *key,
                                struct object *method) {
  int i;
  for (i = 0; i < self->tally; ++i)
    if (key == self->keys[i])
      return self->values[i] = (struct object *)method;
  if (self->tally == self->size) {
    self->size *= 2;
    self->keys = (struct object **)realloc(self->keys, sizeof(struct object *) *
                                                           self->size);
    self->values = (struct object **)realloc(
        self->values, sizeof(struct object *) * self->size);
  }
  self->keys[self->tally] = key;
  self->values[self->tally++] = method;
  return method;
}
static struct object *vtable_lookup(struct vtable *self, struct object *key) {
  int i;
  for (i = 0; i < self->tally; ++i)
    if (key == self->keys[i])
      return self->values[i];
  if (self->parent)
    return send(self->parent, s_lookup, key);
  fprintf(stderr, "lookup failed %p %s\n", self,
          ((struct symbol *)key)->string);
  return 0;
}

static struct object *symbol_intern(struct object *self, char *string) {
  struct object *symbol;
  int i;
  for (i = 0; i < SymbolList->tally; ++i) {
    symbol = SymbolList->keys[i];
    if (!strcmp(string, ((struct symbol *)symbol)->string))
      return symbol;
  }
  symbol = symbol_new(string);
  vtable_addMethod(SymbolList, symbol, 0);
  return symbol;
}
#define trace() printf("%s %d\n", __FUNCTION__, __LINE__); fflush(stdout);

// When id is compiled into brn, it will be loaded here
int LoadID() {
  printf("Loading ID...");
  metavtable = vtable_delegated(0);
  metavtable->_vt[-1] = metavtable;
  object_vt = vtable_delegated(0);
  object_vt->_vt[-1] = metavtable;
  metavtable->parent = object_vt;
  symbol_vt = vtable_delegated(object_vt);
  SymbolList = vtable_delegated(0);
  s_lookup = symbol_intern(0, "lookup");
  vtable_addMethod(metavtable, s_lookup, (struct object *)vtable_lookup);
  s_addMethod = symbol_intern(0, "addMethod");
  vtable_addMethod(metavtable, s_addMethod, (struct object *)vtable_addMethod);
  s_allocate = symbol_intern(0, "allocate");
  send(metavtable, s_addMethod, s_allocate, vtable_allocate);
  symbol = send(symbol_vt, s_allocate, sizeof(struct symbol));
  s_intern = symbol_intern(0, "intern");
  send(symbol_vt, s_addMethod, s_intern, symbol_intern);
  s_delegated = send(symbol, s_intern, (struct object *)"delegated");

  send(metavtable, s_addMethod, s_delegated, vtable_delegated);
  printf("Done.\n");
  return 0;
}

static void entrypoint(void *state) {
  printf("In entrypoint for id\n");
  LoadID();
}

int main() {
  entrypoint(NULL);
  return 0;
}
