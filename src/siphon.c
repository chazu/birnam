// Siphon -

#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
#include <readline/readline.h>
#include <readline/history.h>

int main() {
  void (*fptr)();
  void *handle;
  char *error;

  char token_delim[] = " ";
  char *prompt = ">> ";
  char *input;

  int done = 0;

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
