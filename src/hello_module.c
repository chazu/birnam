/* hello_module.c - A trivial test module for birnam */

#include "id.h"
#include <stdio.h>

/* Global reference to libid API - will be passed to us */
static struct __libid *libid = NULL;

/* Simple hello method - just prints a message */
static oop hello_greet(oop closure, oop state, oop self)
{
  printf("[HELLO] Greetings from the hello module!\n");
  return self;
}

/* Module initialization function */
/* This is called when the module is loaded */
void hello_module_init(struct __libid *_libid)
{
  oop hello_proto;
  oop greet_selector;

  printf("[HELLO MODULE] Initializing...\n");

  /* Store the libid API reference */
  libid = _libid;

  /* Create a new prototype object for Hello */
  hello_proto = libid->proto(libid->_object);

  /* Intern the 'greet' selector */
  greet_selector = libid->intern("greet");

  /* Add the greet method to our hello prototype */
  libid->method(hello_proto, greet_selector, (_imp_t)hello_greet);

  /* Export our hello prototype so other code can access it */
  libid->export("Hello", hello_proto);

  printf("[HELLO MODULE] Initialized! Hello prototype exported.\n");
}
