// System
#define KGFLAGS_IMPLEMENTATION
#include <kgflags.h>

#include <dlfcn.h>
#include <libelf.h>
#include <libtcc.h>
//#include <readline/history.h>
//#include <readline/readline.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

void listSymbols() {
  Elf *e;
  Elf_Kind ek;

  int fd

}
// Loads symbols from shared object file
// TODO probably want to return a handle to the dl
void *loadSymbols(char *str) {
  void *handle;
  char *error;

  printf("[SYS] Loading: %s\n", str);
  handle = dlopen(str, RTLD_LAZY);
  error = dlerror();

  if (error != NULL) {
    printf("[SYS] Error: %s\n", error);
  } else {
    printf("[SYS] Load OK.\n");
  }
}

int compileString(char *str) {
  TCCState *s;
  int (*func)(int);

  s = tcc_new();
  if (!s) {
    fprintf(stderr, "Could not create tcc state\n");
    exit(1);
  }

  /*The output type must be set before any compilation*/
  tcc_set_output_type(s, TCC_OUTPUT_EXE);
  if (tcc_compile_string(s, str) == -1)
    return 1;

  return 0;
}

int main(int argc, char **argv) {
  const char *compile_flag = NULL;
  bool repl_mode = false;

  kgflags_string("compile", NULL, "Unit to compile", false, &compile_flag);
  kgflags_bool("repl", true, "Run REPL", false, &repl_mode);

  if (!kgflags_parse(argc, argv)) {
    kgflags_print_errors();
    kgflags_print_usage();
    return 1;
  }

  int c;

  void (*fptr)();
  void *handle;
  char *error;

  char token_delim[] = " ";
  char *prompt = ">> ";
  char *input;

  int done = 0;

  /************/
  /* load obj */
  /************/
  // TODO handle relative paths lol
  loadSymbols("/home/chazu/dev/c/birnam/bin/obj.so");

  /* if (options.compile_mode) { */
  /* } */

  //
  // Commence the REPLin'
  //
  while (!done) {
    input = readline(prompt);

    if (input == NULL) {
      // Handle ctrl-d
      printf("\n");
      break;
    } else {
      add_history(input);
    }

    char *token;
    token = strtok(input, token_delim);

    if (strcmp(token, "load") == 0) {

      token = strtok(NULL, token_delim);
      // TODO Ensure token exists
      printf("[SYS] Loading: %s\n", token);
      handle = dlopen(token, RTLD_LAZY);
      error = dlerror();

      if (error != NULL) {
        printf("[SYS] Error: %s\n", error);
      } else {
        printf("[SYS] Load OK.\n");
        // TODO enumerate symbols in library
      }
    } else if (strcmp(token, "compile") == 0) {

      token = strtok(NULL, token_delim);

      FILE *f = fopen(token, "ro");
      char *buffer = NULL;
      size_t len;
      ssize_t bytes_read = getdelim(&buffer, &len, '\0', f);

      if (bytes_read != -1) {
        printf("Successfully read info buffer %ld bytes...", bytes_read);
        compileString(buffer);
      }

      return 0;

      /* *(void **)(&fptr) = dlsym(handle, token); */
      /* (*fptr)(); */
    } else if (strcmp(token, "call") == 0) {
      token = strtok(NULL, token_delim);

      // TODO Ensure token exists
      *(void **)(&fptr) = dlsym(handle, token);
      (*fptr)();
    } else if (strcmp(token, "show") == 0) {
      const char symbol;
      token = strtok(NULL, token_delim);
      symbol = dlsym(handle, token);
      printf("%s = %s\n", token, symbol);
    }

    free(input);
  }

  return 0;
}
