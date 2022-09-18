// Siphon -

#include <dlfcn.h>
#include <libtcc.h>
//#include <readline/history.h>
//#include <readline/readline.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

typedef struct Options {
  int compile_mode;
  int repl;
} Options;

int compileString(char *str) {
  TCCState *s;
  int (*func)(int);
  void *mem;
  int size;

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
  Options options;
  int c;

  void (*fptr)();
  void *handle;
  char *error;

  char token_delim[] = " ";
  char *prompt = ">> ";
  char *input;

  int done = 0;

  //
  // Parse options into the options object
  //
  while ((c = getopt(argc, argv, "c:")) != -1) {
    int this_option_optind = optind ? optind : 1;
    switch (c) {
    case 'c':
      printf("Compiling %s\n", optarg);
      options.compile_mode = 1;
      break;
    }
  }

  if (options.compile_mode) {
  }
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
      printf("[SIPHON] Loading: %s\n", token);
      handle = dlopen(token, RTLD_LAZY);
      error = dlerror();

      if (error != NULL) {
        printf("[SIPHON] Error: %s\n", error);
      } else {
        printf("[SIPHON] Load OK.\n");
        // TODO enumerate symbols in library
      }
    } else if (strcmp(token, "compile") == 0) {

      token = strtok(NULL, token_delim);

      FILE *f = fopen(token, "ro");
      char *buffer = NULL;
      size_t len;
      ssize_t bytes_read = getdelim(&buffer, &len, '\0', f);

      if (bytes_read != -1) {
        printf("Successfully read info buffer...");
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
