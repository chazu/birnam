
// /home/chazu/dev/c/birnam/bin/obj.so

// System
#include <dlfcn.h>
#include <libtcc.h>
#include <mach-o/dyld.h>
#include <mach-o/getsect.h>
#include <mach-o/nlist.h>
#include <readline/history.h>
#include <readline/readline.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

// Loads symbols from shared object file
void *load_symbols(char *str) {
  void *handle;
  char *error;

  printf("[SYS] Loading: %s\n", str);
  handle = dlopen(str, RTLD_LAZY | RTLD_GLOBAL);
  error = dlerror();

  if (error != NULL) {
    printf("[SYS] Error: %s\n", error);
  } else {
    printf("[SYS] Load OK.\n");
  }

  return handle;
}

// Iterate through symbols in the loaded object
// and print them
void list_symbols(void *handle) {
  unsigned long size;
  handle =
      dlopen("/Users/chazu/dev/c/birnam/bin/obj.so", RTLD_LAZY | RTLD_GLOBAL);

  // Iterate through loaded images to find the corresponding one
  for (uint32_t i = 0; i < _dyld_image_count(); i++) {
    const char *image_name = _dyld_get_image_name(i);
    if (image_name &&
        strcmp(image_name, "/Users/chazu/dev/c/birnam/bin/obj.so") == 0) {
      const struct mach_header_64 *header =
          (const struct mach_header_64 *)_dyld_get_image_header(i);
      if (!header) {
        fprintf(stderr, "Failed to get Mach-O header.\n");
        dlclose(handle);
        return;
      }

      const struct load_command *lc =
          (const struct load_command *)((char *)header +
                                        sizeof(struct mach_header_64));
      struct symtab_command *symtab = NULL;

      // Iterate over load commands to find LC_SYMTAB
      for (uint32_t j = 0; j < header->ncmds; j++) {
        if (lc->cmd == LC_SYMTAB) {
          symtab = (struct symtab_command *)lc;
          break;
        }
        lc = (const struct load_command *)((char *)lc + lc->cmdsize);
      }

      if (!symtab) {
        fprintf(stderr, "Failed to find symbol table in Mach-O file.\n");
        dlclose(handle);
        return;
      }

      // Get pointers to the symbol and string tables
      const struct nlist_64 *symbols =
          (const struct nlist_64 *)((char *)header + symtab->symoff);
      const char *strtab = (const char *)((char *)header + symtab->stroff);

      // Print each symbol
      for (uint32_t k = 0; k < symtab->nsyms; k++) {
        if (symbols[k].n_un.n_strx != 0) {
          printf("%s\n", strtab + symbols[k].n_un.n_strx);
        }
      }
      break;
    }
  }
}

void hex_dump(const void *addr, size_t length) {
    const unsigned char *ptr = (const unsigned char *)addr;
    for (size_t i = 0; i < length; i += 16) {
        printf("%08lx  ", (unsigned long)(ptr + i));
        for (size_t j = 0; j < 16 && i + j < length; j++) {
            printf("%02x ", ptr[i + j]);
        }
        printf(" ");
        for (size_t j = 0; j < 16 && i + j < length; j++) {
            unsigned char c = ptr[i + j];
            printf("%c", (c >= 32 && c <= 126) ? c : '.');
        }
        printf("\n");
    }
}

void get_symbol_info(void *handle, const char *symbol_name) {
  printf("Searching for symbol '%s' in shared object: %p\n", symbol_name,
         handle);

  // Iterate through loaded images to find the corresponding one
  for (uint32_t i = 0; i < _dyld_image_count(); i++) {
    const char *image_name = _dyld_get_image_name(i);
    if (image_name && strcmp(image_name, "/Users/chazu/dev/c/birnam/bin/obj.so") == 0) {
      const struct mach_header_64 *header =
          (const struct mach_header_64 *)_dyld_get_image_header(i);
      if (!header) {
        fprintf(stderr, "Failed to get Mach-O header.\n");
        dlclose(handle);
        return;
      }

      const struct load_command *lc =
          (const struct load_command *)((char *)header +
                                        sizeof(struct mach_header_64));
      struct symtab_command *symtab = NULL;

      // Iterate over load commands to find LC_SYMTAB
      for (uint32_t j = 0; j < header->ncmds; j++) {
        if (lc->cmd == LC_SYMTAB) {
          symtab = (struct symtab_command *)lc;
          break;
        }
        lc = (const struct load_command *)((char *)lc + lc->cmdsize);
      }

      if (!symtab) {
        fprintf(stderr, "Failed to find symbol table in Mach-O file.\n");
        dlclose(handle);
        return;
      }

      // Get pointers to the symbol and string tables
      const struct nlist_64 *symbols =
          (const struct nlist_64 *)((char *)header + symtab->symoff);
      const char *strtab = (const char *)((char *)header + symtab->stroff);

      // Search for the specific symbol
      for (uint32_t k = 0; k < symtab->nsyms; k++) {
        if (symbols[k].n_un.n_strx != 0) {
          const char *name = strtab + symbols[k].n_un.n_strx;
          if (strcmp(name, symbol_name) == 0) {
            printf("Symbol found: %s\n", name);
            printf("Address: 0x%llx\n", symbols[k].n_value);
            printf("Size: %llu\n", (unsigned long long)symbols[k].n_value);

            // Calculate the absolute address of the symbol
            void *absolute_address = (void *)((uintptr_t)symbols[k].n_value + (uintptr_t)header);

            //hex_dump(absolute_address, symbols[k].n_value);

            return;
          }
        }
      }

      fprintf(stderr, "Symbol '%s' not found in shared object.\n", symbol_name);
      break;
    }
  }

  dlclose(handle);
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
  handle = load_symbols("/Users/chazu/dev/c/birnam/bin/obj.so");

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
      // add_history(input);
    }

    char *token;
    token = strtok(input, token_delim);

    if (strcmp(token, "!load") == 0) {
      token = strtok(NULL, token_delim);

      load_symbols(token);
    } else if (strcmp(token, "!compile") == 0) {

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
    } else if (strcmp(token, "!call") == 0) {
      token = strtok(NULL, token_delim);

      // TODO Ensure token exists
      *(void **)(&fptr) = dlsym(handle, token);
      (*fptr)();
    } else if (strcmp(token, "!show") == 0) {
      const char symbol;
      token = strtok(NULL, token_delim);
      get_symbol_info(handle, token);
    } else if (strcmp(token, "!list") == 0) {
      list_symbols(handle);
    } else if (strcmp(token, "!exit") == 0) {
      done = 1;
    } else {
      printf("Unknown command: %s\n", token);
    }

    free(input);
  }

  return 0;
}
