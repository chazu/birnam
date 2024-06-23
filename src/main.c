#include <dlfcn.h>
#include <stdio.h>
#include <stdlib.h>

#include "libtcc.h"

typedef void* module_entrypoint(void* data);

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

int load_module(char *name, void *state) {
    void *module = dlopen(name, RTLD_NOW);
    module_entrypoint *module_main = dlsym(module, "entrypoint");
    module_main(state);
}

int main(void) {
  // State is whatever we want to persist across the reload
  void *state = NULL;

  // Load ourselves, find the entrypoint and call it.
  load_module("brn", state);
  load_module("peg", state);
  load_module("testgrammar", state);
}

void entrypoint(void *data) {
  printf("BIRNAM --\n");

  // Load object system
  loadId();
  entrypoint_peg();
}
