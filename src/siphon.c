// Siphon -

#include <dlfcn.h>
#include <stdio.h>
#include <string.h>
#include <readline/readline.h>

int main() {
  void (*fptr)();
  void *handle;
  char *error;

  char target_path[100];
  char input_buffer[100];
  char symbol_name[100];

  char token_delim[] = " ";
  char *prompt = "=> ";

  printf("[SIPHON] Starting...\n");

  while (1) {

    char *input = readline(prompt);

    if (input == NULL) {
      // Handle ctrl-d
      printf("\n");
      break;
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
        printf("[SIPHON] No Error loading target.\n");
        // TODO enumerate symbols in library
      }
    } else if (strcmp(token, "call") == 0) {
      token = strtok(NULL, token_delim);

      // TODO Ensure token exists
      *(void **)(&fptr) = dlsym(handle, token);
      (*fptr)();
    }

    /* printf(input); */
    /* printf("\n\n"); */
  }
  /* gets(target_path); */
  /* handle = dlopen(target_path, RTLD_LAZY); */
  /* error = dlerror(); */

  /* if (error != NULL) { */
  /*   printf("[SIPHON] Error: %s\n", error); */
  /* } else { */
  /*   printf("[SIPHON] No Error loading target.\n"); */
  /* } */

  /* // *(void **)(&fptr) = dlsym(handle, "begin"); */
  /* // (*fptr)(); */
  printf("[SIPHON] Done...\n");
  return 0;
}
