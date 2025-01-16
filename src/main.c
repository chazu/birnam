#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>
#include "libtcc.h"
#include <dynload.h>

typedef void *module_entrypoint(void *data);

int compileString(char *str, char *outputFile) {
  TCCState *s;
  int (*func)(int);

  s = tcc_new();
  if (!s) {
    fprintf(stderr, "Could not create tcc state\n");
    exit(1);
  }

  /*The output type must be set before any compilation*/
  tcc_set_output_type(s, TCC_OUTPUT_DLL);
  if (tcc_compile_string(s, str) == -1)
    return 1;

  tcc_output_file(s, outputFile);
  return 0;
}

/* // Use dynload to load a module and list its symbols */
/* void list_symbols(void *handle) { */
/*   int numSyms = dlSymsCount(handle); */
/*   printf("Number of symbols: %d\n", numSyms); */
/*   for (int i=0; i <= numSyms; i++) { */
/*         char *sym = dlSymsName(handle, i); */
/*         printf("Symbol %d: %s\n", i, sym); */
/*   } */
/* } */

// A module is just a dynamically linkable binary file
// with a function called entrypoint that takes an opaque
// pointer containing the state of the running system.
// What that state looks like and is, beyond obviously
// the currently loaded modules, vtables, message passing
// system, we don't know yet.
void load_module(char *name, void *state) {
  char *symName = "entrypoint";
  printf("Loading module %s\n", name);
  void *module = dlopen(name, RTLD_NOW);

  if (!module) {
    fprintf(stderr, "Failed to load module %s: %s\n", name, dlerror());
    return;
  }

  module_entrypoint *module_main = dlsym(module, symName);

  if (!module_main) {
    fprintf(stderr, "Failed to find entrypoint in module %s: %s\n", name, dlerror());
    dlclose(module);
    return;
  }

  printf("Invoking module entrypoint...\n");
  module_main(state);
}


void entrypoint(void *state) {
  printf("Birnam Entrypoint\n");
  // Load ourselves, find the entrypoint and call it.
  // load_module("brn", state);
  // load_module("testgrammar", state);
}

int main() {
  printf("Birnam Main\n");

  void *state = 0;
  // State is whatever we want to persist across the reload

  load_module("./bin/id", state);
  /* load_module("peg", state); */
  // entrypoint(state);
  return 0;
}
