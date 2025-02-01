#include <stdarg.h>
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

#define DEBUG(...) \
    fprintf(stderr, "[DEBUG] %s:%d: ", __FUNCTION__, __LINE__); \
    fprintf(stderr, __VA_ARGS__); \
    fprintf(stderr, "\n");

// Define function pointer types

typedef struct object *(*oop)(struct object *receiver, struct object *args);

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

static oop _bind(struct object *rcv, struct object *msg, struct object *args);
static struct object *vtable_at_put(struct vtable *self, struct object *idx, struct object *item);
static struct vtable *vtable_delegated(struct vtable *self);
struct object *_send(struct object *rcv, struct object *msg,
                     struct object *args);

/*
  Outer send function - encapsulates the use of vtables for
  argument passing
*/
struct object *send(struct object **args) {
  trace();
  // TODO if args is less than two, abort!
  uint64_t i = 0;
  struct object *rcv;
  struct object *msg;
  struct vtable * arg_table = vtable_delegated(0);

  if (!args) {
    printf("Error in send - no receiver specified!");
  }
  rcv = *args;
  args++;

  if (!args) {
    printf("Error in send - no message specified!");
  }
  msg = *args;
  args++;

  while (*args != NULL) {
    vtable_at_put(arg_table, (struct object *)i, args++);
    i++;
  }

  // Make sure the vtable pointer is set
  arg_table->_vt[-1] = arg_table;

  return _send(rcv, msg, (struct object *)arg_table);
}

/*
  Send macro which adds null-termination so we dont have to
  use va_list. One possible TODO is enforcing min 2 args
  here
*/
#define send(...) send((struct object *[]){__VA_ARGS__, NULL})

/* The inner send function: all extra arguments must be packed into a (v)table */
struct object *_send(struct object *rcv, struct object *msg, struct object *args) {
  trace();
    // Look up the method pointer.
  oop method = _bind(rcv, msg, args);
  if (!method) {
    raise(SIGTRAP);
    fprintf(stderr, "ERROR in send: method not found for message\n");
    exit(1);
  }

  // Call the method with the receiver and the va_list.
  struct object *result = method(rcv, args);
  return result;
}

// Define primitives for object system
static struct vtable *metavtable = 0;
static struct vtable *object_vt = 0;
static struct vtable *symbol_vt = 0;

static struct object *s_addMethod = 0;
static struct object *s_allocate = 0;
static struct object *s_delegated = 0;
static struct object *s_lookup = 0;
static struct object *s_intern = 0;

// TODO s_atPut for vtable
// TODO s_at for vtable

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
  printf("***VTable %p***\n", vt);
  for (int i = 0; i < vt->tally; i++) {
    printf("  %s : %p\n", ((struct symbol *)vt->keys[i])->string,
           vt->values[i]);
  }
  printf("***End VTable %p***\n", vt);
}

static struct object *symbol_new(char *string) {
  trace();
  struct symbol *symbol = (struct symbol *)alloc(sizeof(struct symbol));
  symbol->_vt[-1] = symbol_vt;
  symbol->string = strdup(string);
  return (struct object *)symbol;
}

static struct object *vtable_lookup(struct vtable *self, struct object *key);

static oop _bind(struct object *rcv, struct object *msg, struct object *args) {
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
  ((struct vtable **)object)[-1] = self;
  //object->_vt[-1] = self;
  return object;
}

static struct object *vtable_at_put(struct vtable *self, struct object *idx, struct object *item) {
  DEBUG("self=%p idx=%i item=%p", self, idx, item);
  if (self->tally == self->size) {
    trace();
    self->size *= 2;
    self->keys   = (struct object **)realloc(self->keys, sizeof(struct object *) *self->size);
    self->values = (struct object **)realloc(self->values, sizeof(struct object *) * self->size);
  }

  self->keys[self->tally] = idx;
  self->values[self->tally++] = item;
  return (struct object*)self;
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
  DEBUG("self=%p key=%p", self, key);
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
  trace();
  print_vtable(self);
  for (i = 0; i < self->tally; ++i) {
    trace();
    if (!self->keys[i])
      trace();
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
