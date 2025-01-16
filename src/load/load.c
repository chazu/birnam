/*  load.c - Load symbols from a shared object file  */

#include <dlfcn.h>
#include <mach-o/dyld.h>
#include <mach-o/getsect.h>
#include <mach-o/nlist.h>
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
  printf("Loading symbols here boss\n");
  unsigned long size;

  // Iterate through loaded images to find the corresponding one
  for (uint32_t i = 0; i < _dyld_image_count(); i++) {
    const char *image_name = _dyld_get_image_name(i);
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

void *get_symbol(void *handle, const char *symbol_name) {
    printf("Searching for symbol '%s' in shared object: %p\n", symbol_name,
         handle);

  // Iterate through loaded images to find the corresponding one
  for (uint32_t i = 0; i < _dyld_image_count(); i++) {
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
          return (void *)symbols[k].n_value;
        }
      }
    }
  }
}

void get_symbol_info(void *handle, const char *symbol_name) {
  printf("Searching for symbol '%s' in shared object: %p\n", symbol_name,
         handle);

  // Iterate through loaded images to find the corresponding one
  for (uint32_t i = 0; i < _dyld_image_count(); i++) {
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
          void *absolute_address =
              (void *)((uintptr_t)symbols[k].n_value + (uintptr_t)header);

          // hex_dump(absolute_address, symbols[k].n_value);

          return;
        }
      }
    }

    fprintf(stderr, "Symbol '%s' not found in shared object.\n", symbol_name);
    break;
  }
}
