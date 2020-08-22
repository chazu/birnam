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

  char target_path[100];
  char input_buffer[100];
  char symbol_name[100];

  char token_delim[] = " ";
  char *prompt = ">> ";
  char *input;

  int done = 0;
  printf("[SIPHON] Starting...\n");

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
    }

    free(input);
  }

  printf("[SIPHON] Done...\n");
  return 0;
}
