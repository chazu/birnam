
// Birnam System - loads and initializes the libid object system

// System includes
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

// libid object system
#include "../id.h"
#include "../id_internal.h"

// Global libid API
struct __libid *libid = NULL;

// System object state
typedef struct {
  oop _vtable[0];  // Object header
  int exit_requested;
  void *modules[32]; // Track loaded module handles
  int module_count;
} System;

// Global system instance
static oop system_obj = NULL;

/************************/
/* Message-passing helpers */
/************************/

/* Helper to send unary messages */
static oop send_message(oop receiver, const char *selector_name)
{
  oop sel = libid->intern(selector_name);
  struct __closure *closure = libid->bind(sel, receiver);
  if (!closure) return NULL;
  return closure->method((oop)closure, receiver, receiver);
}

/* Helper to send messages with one argument */
static oop send_message_with_arg(oop receiver, const char *selector_name, oop arg)
{
  oop sel = libid->intern(selector_name);
  struct __closure *closure = libid->bind(sel, receiver);
  if (!closure) return NULL;
  return closure->method((oop)closure, receiver, receiver, arg);
}

/************************/
/* Symbol loading utilities */
/************************/

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

/************************/
/* System Object Methods */
/************************/

/* System help - show available commands */
static oop system_help(oop closure, oop state, oop self)
{
  printf("\nBirnam System Commands:\n");
  printf("  help                    - Show this help message\n");
  printf("  objects                 - List all exported objects with addresses\n");
  printf("  inspect: <name>         - Inspect object (show type and methods)\n");
  printf("  methodsOf: <name>       - List methods of an object\n");
  printf("  respondsTo: <obj> with: <sel> - Check if object responds to selector\n");
  printf("  list                    - List loaded modules\n");
  printf("  load: <path>            - Load a module from path\n");
  printf("  show: <name>            - Show symbol information\n");
  printf("  exit                    - Exit the system\n");
  printf("\n");
  return self;
}

/* System list - show loaded modules */
static oop system_list(oop closure, oop state, oop self)
{
  System *sys = (System *)self;
  printf("\n[SYSTEM] Loaded modules: %d\n", sys->module_count);
  for (int i = 0; i < sys->module_count; i++) {
    printf("  [%d] %p\n", i, sys->modules[i]);
  }
  printf("\n");
  return self;
}

/* System load: - load a module */
static oop system_load(oop closure, oop state, oop self, oop path_oop)
{
  System *sys = (System *)self;
  void *handle;

  // Extract string from selector oop
  _selector_t *path_sel = (_selector_t *)path_oop;
  const char *path = path_sel->elements;

  printf("[SYSTEM] Loading module: %s\n", path);

  handle = dlopen(path, RTLD_LAZY | RTLD_GLOBAL);
  if (!handle) {
    fprintf(stderr, "[SYSTEM] Error loading module: %s\n", dlerror());
    return NULL;
  }

  // Look for module_init function - convention: <basename>_init
  // For now, just track the handle
  if (sys->module_count < 32) {
    sys->modules[sys->module_count++] = handle;
    printf("[SYSTEM] Module loaded successfully (handle: %p)\n", handle);
  } else {
    fprintf(stderr, "[SYSTEM] Too many modules loaded\n");
    dlclose(handle);
    return NULL;
  }

  return self;
}

/* System show: - show symbol info */
static oop system_show(oop closure, oop state, oop self, oop name_oop)
{
  System *sys = (System *)self;

  // Extract string from selector oop
  _selector_t *name_sel = (_selector_t *)name_oop;
  const char *name = name_sel->elements;

  printf("[SYSTEM] Looking up symbol: %s\n", name);

  // Try to import from libid namespace first
  oop obj = libid->import(name);
  if (obj) {
    printf("[SYSTEM] Found object '%s' at %p\n", name, obj);
    // TODO: Add more introspection capabilities here
  } else {
    printf("[SYSTEM] Symbol '%s' not found in object system\n", name);

    // Try dlsym on loaded modules
    for (int i = 0; i < sys->module_count; i++) {
      void *sym = dlsym(sys->modules[i], name);
      if (sym) {
        printf("[SYSTEM] Found C symbol '%s' in module %d at %p\n", name, i, sym);
        break;
      }
    }
  }

  return self;
}

/* System exit - request exit */
static oop system_exit(oop closure, oop state, oop self)
{
  System *sys = (System *)self;
  printf("[SYSTEM] Exit requested\n");
  sys->exit_requested = 1;
  return self;
}

/* System getLibid - return libid API reference */
static oop system_getLibid(oop closure, oop state, oop self)
{
  // Return libid as an oop (this is a bit of a hack, but useful for modules)
  return (oop)libid;
}

/* System objects - list all exported objects */
static oop system_objects(oop closure, oop state, oop self)
{
  printf("\n[SYSTEM] Exported objects:\n");

  // Access _object_Table directly from the libid.so handle
  void *libid_handle = dlopen("./bin/libid.so", RTLD_LAZY | RTLD_NOLOAD);
  if (!libid_handle) {
    printf("  Error: Cannot access libid internals\n");
    return self;
  }

  _vtable_t **object_table_ptr = dlsym(libid_handle, "_object_Table");
  if (!object_table_ptr || !*object_table_ptr) {
    printf("  Error: Cannot find _object_Table\n");
    return self;
  }

  _vtable_t *object_table = *object_table_ptr;
  _vector_t *bindings = (_vector_t *)object_table->bindings;

  printf("  Total exports: %zu\n\n", object_table->tally);

  for (size_t i = 0; i < object_table->tally; i++) {
    _assoc_t *assoc = (_assoc_t *)bindings->elements[i];
    if (assoc) {
      _selector_t *key = (_selector_t *)assoc->key;
      printf("  %-20s -> %p\n", key->elements, assoc->value);
    }
  }

  printf("\n");
  return self;
}

/* System inspect: - object introspection using pure message passing */
static oop system_inspect(oop closure, oop state, oop self, oop name_oop)
{
  _selector_t *name_sel;
  const char *name;
  oop obj;

  if (!name_oop) {
    printf("\n  Error: NULL name\n\n");
    return self;
  }

  // Extract string from selector oop
  name_sel = (_selector_t *)name_oop;
  if (!name_sel || !name_sel->elements) {
    printf("\n  Error: Invalid selector\n\n");
    return self;
  }

  name = name_sel->elements;
  printf("\n[SYSTEM] Inspecting: %s\n", name);

  // Check if exported
  if (!libid->isExported(name)) {
    printf("  Status:   NOT EXPORTED\n\n");
    return self;
  }

  // Import the object
  obj = libid->import(name);
  printf("  Status:   Exported\n");
  printf("  Address:  %p\n\n", obj);

  // Send 'describe' message to the object - pure message passing!
  send_message(obj, "describe");

  printf("\n");
  return self;
}

/* System methodsOf: - list methods of an exported object */
static oop system_methodsOf(oop closure, oop state, oop self, oop name_oop)
{
  _selector_t *name_sel = (_selector_t *)name_oop;
  const char *name = name_sel->elements;

  if (!libid->isExported(name)) {
    printf("  Object '%s' not found\n", name);
    return self;
  }

  oop obj = libid->import(name);

  // Send 'methods' message to get vector of selectors
  oop methods_vec = send_message(obj, "methods");

  if (!methods_vec) {
    printf("  Object doesn't respond to 'methods'\n");
    return self;
  }

  // Cast to vector (we know what we got back)
  _vector_t *methods = (_vector_t *)methods_vec;

  printf("\nMethods of '%s' (%zu):\n", name, methods->size);
  for (size_t i = 0; i < methods->size; i++) {
    _selector_t *sel = (_selector_t *)methods->elements[i];
    if (sel && sel->elements) {
      printf("  %s\n", sel->elements);
    }
  }
  printf("\n");

  return self;
}

/* System respondsTo:with: - check if object responds to selector */
static oop system_respondsTo_with(oop closure, oop state, oop self, oop name_oop, oop selector_oop)
{
  _selector_t *name_sel = (_selector_t *)name_oop;
  const char *name = name_sel->elements;

  if (!libid->isExported(name)) {
    printf("  Object '%s' not found\n", name);
    return (oop)0;
  }

  oop obj = libid->import(name);

  // Send 'respondsTo:' message with the selector as argument
  oop result = send_message_with_arg(obj, "respondsTo:", selector_oop);

  _selector_t *sel = (_selector_t *)selector_oop;
  printf("  %s %s to '%s'\n",
    name,
    result ? "responds" : "does not respond",
    sel->elements);

  return result;
}

int main(int argc, char **argv, char **envp) {
  void (*fptr)();
  void *handle;
  struct __libid *(*_libid_init)(int *, char ***, char ***);

  char token_delim[] = " ";
  char *prompt = ">> ";
  char *input;
  int done = 0;

  /************************/
  /* Initialize libid     */
  /************************/
  printf("[SYS] Loading libid object system...\n");
  handle = dlopen("./bin/libid.so", RTLD_LAZY | RTLD_GLOBAL);
  if (!handle) {
    fprintf(stderr, "[SYS] Failed to load libid.so: %s\n", dlerror());
    return 1;
  }

  _libid_init = dlsym(handle, "_libid_init");
  if (!_libid_init) {
    fprintf(stderr, "[SYS] Failed to find _libid_init: %s\n", dlerror());
    return 1;
  }

  printf("[SYS] Initializing libid...\n");
  libid = _libid_init(&argc, &argv, &envp);
  if (!libid) {
    fprintf(stderr, "[SYS] libid initialization failed\n");
    return 1;
  }

  printf("[SYS] libid initialized successfully!\n");
  printf("[SYS] libid API at %p\n", libid);
  printf("[SYS] _object prototype at %p\n", libid->_object);

  /************************/
  /* Create System Object */
  /************************/
  printf("\n[SYS] Creating System object...\n");

  // Create System prototype
  oop system_proto = libid->proto(libid->_object);

  // Allocate System instance with our custom struct size
  system_obj = libid->alloc(system_proto, sizeof(System));
  System *sys = (System *)system_obj;
  sys->exit_requested = 0;
  sys->module_count = 0;

  // Register System methods
  libid->method(system_proto, libid->intern("help"), (_imp_t)system_help);
  libid->method(system_proto, libid->intern("objects"), (_imp_t)system_objects);
  libid->method(system_proto, libid->intern("inspect:"), (_imp_t)system_inspect);
  libid->method(system_proto, libid->intern("methodsOf:"), (_imp_t)system_methodsOf);
  libid->method(system_proto, libid->intern("respondsTo:with:"), (_imp_t)system_respondsTo_with);
  libid->method(system_proto, libid->intern("list"), (_imp_t)system_list);
  libid->method(system_proto, libid->intern("load:"), (_imp_t)system_load);
  libid->method(system_proto, libid->intern("show:"), (_imp_t)system_show);
  libid->method(system_proto, libid->intern("exit"), (_imp_t)system_exit);
  libid->method(system_proto, libid->intern("getLibid"), (_imp_t)system_getLibid);

  // Export System object so modules can access it
  libid->export("System", system_obj);

  printf("[SYS] System object created at %p\n", system_obj);
  printf("[SYS] System exported to object system\n");

  // Test: send 'help' message to System
  printf("\n[SYS] Testing System object...\n");
  oop help_sel = libid->intern("help");
  struct __closure *help_closure = libid->bind(help_sel, system_obj);
  help_closure->method((oop)help_closure, system_obj, system_obj);

  /************************/
  /* Test loading a module */
  /************************/
  printf("\n[SYS] Testing module loading...\n");

  void *hello_handle = dlopen("./bin/hello_module.so", RTLD_LAZY | RTLD_GLOBAL);
  if (!hello_handle) {
    fprintf(stderr, "[SYS] Failed to load hello_module.so: %s\n", dlerror());
  } else {
    void (*hello_init)(struct __libid *) = dlsym(hello_handle, "hello_module_init");
    if (!hello_init) {
      fprintf(stderr, "[SYS] Failed to find hello_module_init: %s\n", dlerror());
    } else {
      /* Call module init, passing libid API */
      hello_init(libid);

      /* Try to import the Hello object */
      oop hello_obj = libid->import("Hello");
      if (!hello_obj) {
        fprintf(stderr, "[SYS] Failed to import 'Hello' from module\n");
      } else {
        printf("[SYS] Successfully imported Hello object at %p\n", hello_obj);

        /* Try to send 'greet' message to Hello */
        oop greet_sel = libid->intern("greet");
        struct __closure *greet_closure = libid->bind(greet_sel, hello_obj);
        if (!greet_closure) {
          fprintf(stderr, "[SYS] Failed to bind 'greet' method\n");
        } else {
          printf("[SYS] Calling greet method...\n");
          /* Call the method: closure->method(closure, state, receiver) */
          greet_closure->method((oop)greet_closure, hello_obj, hello_obj);
          printf("[SYS] Method call succeeded!\n");
        }
      }
    }
  }
  printf("\n");

  //
  // Commence the REPLin'
  //
  printf("\n[SYS] Entering REPL (send 'help' for commands)...\n\n");

  while (!sys->exit_requested) {
    input = readline(prompt);

    if (input == NULL) {
      // Handle ctrl-d
      printf("\n");
      break;
    }

    // Skip empty input
    if (strlen(input) == 0) {
      free(input);
      continue;
    }

    // Parse command and argument manually
    char *command = input;
    char *arg = NULL;

    // Find first space
    char *space = strchr(input, ' ');
    if (space) {
      *space = '\0';  // Terminate command
      arg = space + 1;

      // Trim leading and trailing whitespace from arg
      // Trim leading
      while (*arg == ' ' || *arg == '\t') arg++;

      // Trim trailing
      if (strlen(arg) > 0) {
        char *end = arg + strlen(arg) - 1;
        while (end >= arg && (*end == ' ' || *end == '\t' || *end == '\n' || *end == '\r')) {
          *end = '\0';
          end--;
        }
      }

      // If arg is now empty, set it to NULL
      if (strlen(arg) == 0) {
        arg = NULL;
      }
    }

    if (!command || strlen(command) == 0) {
      free(input);
      continue;
    }

    // Map old-style !commands to selectors
    if (command[0] == '!') {
      command++;  // Skip the '!'
    }

    // Determine selector based on whether we have an argument
    char selector[256];
    size_t cmd_len = strlen(command);
    int has_colon = (cmd_len > 0 && command[cmd_len - 1] == ':');

    if (arg && strlen(arg) > 0) {
      // Single-argument message
      if (has_colon) {
        // User already typed the colon: "inspect: arg"
        snprintf(selector, sizeof(selector), "%s", command);
      } else {
        // Add colon: "load arg" -> "load:"
        snprintf(selector, sizeof(selector), "%s:", command);
      }
    } else {
      // Unary message: remove trailing colon if present
      if (has_colon) {
        command[cmd_len - 1] = '\0';  // Remove the colon
      }
      snprintf(selector, sizeof(selector), "%s", command);
    }

    // Intern the selector
    oop sel = libid->intern(selector);

    // Bind the selector to system object
    struct __closure *closure = libid->bind(sel, system_obj);

    if (!closure) {
      printf("Unknown command: %s (try 'help')\n", command);
      free(input);
      continue;
    }

    // Call the method
    if (arg && strlen(arg) > 0) {
      // Intern the string argument to convert to oop
      oop arg_oop = libid->intern(arg);
      // Call with argument - cast to typed function pointer to avoid variadic calling convention issues
      typedef oop (*method_1arg_t)(oop, oop, oop, oop);
      ((method_1arg_t)closure->method)((oop)closure, system_obj, system_obj, arg_oop);
    } else {
      // Call without argument
      closure->method((oop)closure, system_obj, system_obj);
    }

    free(input);
  }

  printf("\n[SYS] Goodbye!\n");

  return 0;
}
