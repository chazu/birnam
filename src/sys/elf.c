#include <stdio.h>

#include <stdio.h>
#include <stdlib.h>
#include <inttypes.h>

#include <dynload.h>


int listSymbols(const char *path) {
  printf("Path: %s\n", path);
  DLLib *lib   = dlLoadLibrary(path);
  DLSyms *syms = dlSymsInit(path);
  int numSyms  = dlSymsCount(syms);

  for (int i = 0; i < numSyms; i++) {
    printf("Symbol: %s\n", dlSymsName(syms, i));
  }
}
