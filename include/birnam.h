#ifndef BIRNAM_H
#define BIRNAM_H

/* The seed's message boundary. Every argument and result is an Oop. */
typedef struct BirnamObject *Oop;
typedef Oop (*BirnamEntry)(Oop method, Oop receiver, Oop arguments);

Oop birnam_bootstrap(void);
Oop birnam_send(Oop receiver, Oop selector, Oop arguments);
Oop birnam_bind(Oop receiver, Oop selector);
Oop birnam_enter(Oop method, Oop receiver, Oop arguments);
Oop birnam_intern(Oop name_bytes);

/* Mechanical bootstrap anchors; these are objects in the same world. */
Oop birnam_object_behavior(void);
Oop birnam_bytes_behavior(void);
Oop birnam_vector_behavior(void);
Oop birnam_method_behavior(void);

/* A module exports this exact C entry. receiver is the live Namespace;
 * arguments is the empty argument Vector. A returned Error fails the load.
 * Methods published before a failure remain installed; no rollback is implied.
 */
Oop birnam_module_init_v1(Oop method, Oop receiver, Oop arguments);

#endif
