#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

struct vtable;
struct object;
struct symbol;

#define trace()                                                                \
  printf("%s %d\n", __FUNCTION__, __LINE__);                                   \
  fflush(stdout);

// Define function pointer types

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

#define send(RCV, MSG, ...)                                                    \
  ({                                                                           \
    struct object *r = (struct object *)RCV;                                   \
    oop method = _bind(r, MSG);                                              \
    if (!method) {                       \
      raise(SIGTRAP); \
      printf("ERROR in send: \n");       \
      exit(1);                           \
    };                                   \
    method(r, __VA_ARGS__);            \
  })

/* oop send(void *RCV, void *MSG, ...) { */
/*     struct object *r = (struct object *)RCV; */
/*     oop method = _bind(r, (MSG)); */
/*     method(r, __VA_ARGS__); */
/* } */

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
  if (size == 0)
    return NULL;
  struct vtable **ppvt =
      (struct vtable **)calloc(1, sizeof(struct vtable *) + size);
  if (!ppvt) {
    fprintf(stderr, "Memory allocation failed\n");
    exit(1);
  }
  return (void *)(ppvt + 1);
}

void print_vtable(struct vtable *vt) {
  printf("VTable %p\n", vt);
  for (int i = 0; i < vt->tally; i++) {
    printf("  %s : %p\n", ((struct symbol *)vt->keys[i])->string,
           vt->values[i]);
  }
  printf("End VTable %p\n", vt);
}

static struct object *symbol_new(char *string) {
  trace();
  struct symbol *symbol = (struct symbol *)alloc(sizeof(struct symbol));
  symbol->_vt[-1] = symbol_vt;
  symbol->string = strdup(string);
  return (struct object *)symbol;
}

static struct object *vtable_lookup(struct vtable *self, struct object *key);

static oop _bind(struct object *rcv, struct object *msg) {
  trace();
  printf("Bind Operation - message: %s\n", ((struct symbol *)msg)->string);
  if (!rcv) {
    fprintf(stderr, "Null receiver in _bind\n");
    return NULL;
  }
  if (!msg) {
    fprintf(stderr, "Null message in _bind\n");
    return NULL;
  }
  oop method;
  struct vtable *vt = rcv->_vt[-1];
  if ((msg == s_lookup) && (rcv == (struct object *)metavtable)) {
    printf("in bind, bottoming out with lookup for message %s\n", ((struct symbol *)msg)->string);
    method = (oop)vtable_lookup(vt, msg);
  } else {
    printf("In bind, About to send lookup message for message %s\n", ((struct symbol *)msg)->string);
    print_vtable(vt);
    method = (oop)send(vt, s_lookup, msg);
  }
  return method;
}

static struct object *vtable_allocate(struct vtable *self, int payloadSize) {
  trace();
  struct object *object = (struct object *)alloc(payloadSize);
  object->_vt[-1] = self;
  return object;
}

static struct vtable *vtable_delegated(struct vtable *self) {
  trace();
  struct vtable *child = (struct vtable *)vtable_allocate(self, sizeof(struct vtable));
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
  trace();
  int i;
  for (i = 0; i < self->tally; ++i)
    if (key == self->keys[i])
      return self->values[i] = (struct object *)method;
  if (self->tally == self->size) {
    self->size *= 2;
    self->keys   = (struct object **)realloc(self->keys, sizeof(struct object *) *self->size);
    self->values = (struct object **)realloc(self->values, sizeof(struct object *) * self->size);
  }

  self->keys[self->tally] = key;
  self->values[self->tally++] = method;
  return method;
}
static struct object *vtable_lookup(struct vtable *self, struct object *key) {
  trace();
  int i;
  printf("===Performing VTable Lookup===\n");
  if (!self) {
    fprintf(stderr, "null self pointer in vtable_lookup\n");
    return 0;
  }

  if (!key) {
    fprintf(stderr, "null key pointer in vtable_lookup\n");
    return 0;
  }

  print_vtable(self);
  for (i = 0; i < self->tally; ++i) {
    if (!self->keys[i])
      continue;
    // print current key and target key
    printf("  Target key %s\n", ((struct symbol *)key)->string);
    printf("  Current key %s\n", ((struct symbol *)self->keys[i])->string);
    if (key == self->keys[i]) {
      printf("  Target key found! returning...\n");
      printf("===VTable Lookup Complete===\n");
      return self->values[i];
    }
  }

  if (self->parent) {
    return send(self->parent, s_lookup, key);
  }

  fprintf(stderr, "lookup failed %p %s\n", self,
          ((struct symbol *)key)->string);
  return 0;
}

static struct object *symbol_intern(struct object *self, char *string) {
  trace();
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

// When id is compiled into brn, it will be loaded here
static int LoadID() {
  printf("Loading ID...\n");
  metavtable = vtable_delegated(0);
  printf("metavtable after vtable_delegated: %p\n", metavtable);

  metavtable->_vt[-1] = metavtable;
  printf("metavtable: %p\n", metavtable);
  object_vt = vtable_delegated(0);
  object_vt->_vt[-1] = metavtable;
  metavtable->parent = object_vt;

  symbol_vt = vtable_delegated(object_vt);
  SymbolList = vtable_delegated(0);

  s_lookup = symbol_intern(0, "lookup");
  s_addMethod = symbol_intern(0, "addMethod");
  s_allocate = symbol_intern(0, "allocate");
  s_intern = symbol_intern(0, "intern");

  vtable_addMethod(metavtable, s_lookup, (struct object *)vtable_lookup);
  vtable_addMethod(metavtable, s_addMethod, (struct object *)vtable_addMethod);

  printf("NOW THE PAIN BEGINS\n");
  // TODO addMethod is not returning its implementation from call to _bind in send on next line
  send(metavtable, s_addMethod, s_allocate, vtable_allocate);
  /* printf("Now the pain should be over\n"); */
  /* symbol = send(symbol_vt, s_allocate, sizeof(struct symbol)); */

  /* send(symbol_vt, s_addMethod, s_intern, symbol_intern); */
  /* s_delegated = send(symbol, s_intern, (struct object *)"delegated"); */

  /* send(metavtable, s_addMethod, s_delegated, vtable_delegated); */
  /* printf("Done.\n"); */
  /* return 0; */
}

void entrypoint(void *state) {
  printf("In entrypoint for id\n");
  LoadID();
}

int main() {
  entrypoint(NULL);
  return 0;
}
