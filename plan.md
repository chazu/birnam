# Birnam Development Plan

## Vision

Birnam is a completely live, completely malleable programming environment inspired by Piumarta's COLA project. The goal is to create a "Smalltalk-like system with first-class PEG parser support" that allows rewriting the language itself within a rich, quasi-image-based programming environment.

Key principles:
- **Everything is an object** communicating via message passing
- **Hot reloading** - modules can be recompiled and reloaded at runtime
- **File-based + image-based** - source stored in files (Git-friendly), but mutable at runtime
- **Self-hosting** - system can recompile itself
- **Network accessible** - nREPL-like interface for external tools

---

## Current State (Phase 0 Complete)

### ✅ What Works Now

**Object System Foundation:**
- ✅ libid_enhanced compiled and integrated (aarch64-compatible, non-variadic macros)
- ✅ libid initialized on system startup
- ✅ Full libid API available (`struct __libid *libid`)
- ✅ Message passing working (intern, bind, method calls)

**System as Object:**
- ✅ System itself is a first-class object in the object system
- ✅ System exported as "System" - accessible to all modules
- ✅ REPL refactored to use message passing (commands → selectors)
- ✅ System protocol defined:
  - `help` - show available commands
  - `list` - list loaded modules
  - `load: <path>` - load a module
  - `show: <name>` - show symbol info
  - `exit` - exit system
  - `getLibid` - return libid API reference

**Module System (Proof of Concept):**
- ✅ Modules can be written in C
- ✅ Module initialization protocol: `module_init(struct __libid *)`
- ✅ Modules use `libid->export(name, oop)` to expose objects
- ✅ Modules use `libid->import(name)` to get objects
- ✅ Example module: `hello_module.c` (creates Hello object with greet method)

**Build System:**
- ✅ Makefile targets for libid, system, modules
- ✅ Proper linking with Boehm GC
- ✅ Platform-specific defines for macOS aarch64

### 📂 File Structure

```
src/
  id.h                  - Core libid types (from idst)
  libid_enhanced.c/h    - Object system implementation (non-variadic)
  hello_module.c        - Example module
  sys/
    system.c            - Main system (now an object)

bin/
  libid.so              - Object system shared library
  system                - Main executable
  hello_module.so       - Example module
```

---

## Architecture

### libid Object Model

**Core Concepts:**
- `oop` - object pointer (typedef for `struct t__object *`)
- `struct __libid` - API vtable with function pointers
- `_vtable` - method dictionary for each prototype
- `_selector` - interned symbol for message names
- `_closure` - method implementation (function pointer + data)

**Object Layout:**
```
Memory:  [_vtable ptr][ object fields... ]
           ^
           |
         oop points here (one word after vtable)
```

**Message Send Flow:**
```
1. libid->intern("selector") → oop selector
2. libid->bind(selector, receiver) → struct __closure *
3. closure->method(closure, state, receiver, args...) → oop result
```

### Module Interface

Modules follow this convention:

```c
#include "id.h"

static struct __libid *libid = NULL;

void <module_name>_init(struct __libid *_libid) {
  libid = _libid;

  // Create prototypes
  oop proto = libid->proto(libid->_object);

  // Add methods
  libid->method(proto, libid->intern("selector"), (_imp_t)my_func);

  // Export objects
  libid->export("MyObject", proto);
}
```

**Key Points:**
- Modules receive libid API during init
- Everything exposed is an `oop`, not a C pointer
- Modules can `import("System")` to interact with the system
- No symbol collisions - everything in object namespace

---

## Next Steps

### Phase 1: Core Module Infrastructure

**Goals:**
- Standardize module interface
- Enable module introspection
- Support module reloading

**Tasks:**
1. **Define Module base prototype**
   - Standard slots: name, version, dependencies, source_path
   - Standard methods: `getName`, `getVersion`, `reload`, `unload`
   - All modules delegate to Module prototype

2. **Module registration protocol**
   - Modules call `System registerModule: self` during init
   - System tracks all registered modules
   - System can query module metadata

3. **Improve System module loading**
   - `System load: path` should:
     - Detect module name from path
     - Find and call `<name>_init` automatically
     - Register module if it implements Module protocol
     - Return the module object

4. **Add module introspection**
   - `System modules` - return collection of all modules
   - `System moduleNamed: "Name"` - get specific module
   - `Module listMethods` - show available methods
   - `Module respondsTo: "selector"` - check if method exists

### Phase 2: Compiler Integration (libtcc wrapper)

**Goals:**
- Expose compilation as message passing
- Enable runtime code generation
- Foundation for hot reloading

**Tasks:**
1. **Create tcc_module.c**
   - Wrap libtcc in object protocol
   - TCC object with methods:
     - `compile: sourceString` - compile to memory
     - `compileFile: path` - compile file to .so
     - `addIncludePath: path`
     - `addLibraryPath: path`
     - `linkLibrary: name`

2. **Integrate with System**
   - `System compile: source` → delegates to TCC module
   - `System compileModule: path` → compile and load in one step

3. **Self-compilation test**
   - System can recompile hello_module.c from source
   - System can reload the recompiled module
   - Verify changes take effect

### Phase 3: Parser Integration (packcc wrapper)

**Goals:**
- PEG parsers as first-class objects
- Runtime grammar modification
- Foundation for language malleability

**Tasks:**
1. **Modify packcc to emit libid-compatible code**
   - Parser generators should produce modules
   - Generated parsers should expose parse methods via object system

2. **Create Parser prototype**
   - Methods: `parse: string`, `parseFile: path`
   - Parsers return AST as objects (not C structs)

3. **Grammar object**
   - Store grammar source as object
   - `Grammar compile` → generates and loads parser module
   - `Grammar modify: source` → update and recompile

### Phase 4: Hot Reloading

**Goals:**
- Reload modules without restarting system
- Preserve state across reloads
- Enable live development workflow

**Tasks:**
1. **State preservation protocol**
   - Modules implement `getState` → returns oop with state
   - Modules implement `setState:` → restores from oop

2. **Module reload workflow**
   - `System reloadModule: "Name"`
     1. Call `module getState` → save state oop
     2. Call `module unload`
     3. dlclose old module handle
     4. Recompile source if changed
     5. dlopen new module handle
     6. Call `new_module_init(libid)`
     7. Call `new_module setState: savedState`
     8. Re-register with System

3. **System self-reload**
   - `System reload` - recompile and reload system.c itself
   - Requires careful state management
   - Potentially save entire object graph first

### Phase 5: Image Persistence

**Goals:**
- Snapshot object system state to disk
- Load saved state on startup
- Support both file-based and image-based workflows

**Tasks:**
1. **Object serialization**
   - Walk object graph from roots
   - Serialize objects to portable format
   - Handle references between objects

2. **Snapshot command**
   - `System snapshot: "filename"` - save state
   - `System loadSnapshot: "filename"` - restore state

3. **Image reconciliation**
   - Detect changes between image and source files
   - Allow choosing which takes precedence
   - Tools to export image changes back to source

### Phase 6: Network REPL

**Goals:**
- Remote access to running system
- Tool integration (editors, debuggers)
- Multi-user/multi-session support

**Tasks:**
1. **TCP server**
   - Listen on configurable port
   - Accept multiple connections
   - Each connection gets own REPL session

2. **Protocol design**
   - Simple line-based or bencode (like nREPL)
   - Commands: eval, inspect, listModules, etc.
   - Responses include success/failure, results, errors

3. **Editor integration**
   - Example: Emacs/VSCode plugin
   - Send expressions for evaluation
   - Get results inline
   - Live feedback during development

---

## Design Decisions & Open Questions

### Module Source Embedding

**Goal:** Compiled modules should contain their source code.

**Options:**
- **A) String literal**: `const char *__module_source = "...";` in generated code
- **B) Custom section**: Use ELF/Mach-O sections to store source
- **C) Separate file**: .so + .src file pair

**Decision needed**: Start with A (simplest), migrate to B if needed?

### Namespace Management

**Current:** Single flat namespace via libid import/export.

**Future considerations:**
- Modules as namespaces: `MyModule::MyClass`?
- Hierarchical namespaces?
- Private vs public exports?

**Decision needed**: Keep simple for now, add namespacing when it becomes a problem?

### Object vs C Function Boundary

**Guideline:** When should something be a C function vs an object method?

**Current approach:**
- System internals (dlopen, etc.) can be C
- Anything exposed to REPL/modules must be object method
- Performance-critical inner loops can stay in C

**Decision:** This seems right. Document the convention.

### Memory Management

**Current:** Using Boehm GC from libid.

**Considerations:**
- GC is conservative, may retain some garbage
- No explicit free() needed
- Works well for now

**Decision:** Keep GC, optimize later if needed.

---

## Success Criteria for MVP

The minimum viable product is achieved when:

1. ✅ Object system (libid) integrated and working
2. ✅ System is a first-class object
3. ✅ Modules can be loaded dynamically
4. ✅ Message passing works throughout
5. ⏳ TCC module enables runtime compilation
6. ⏳ Modules can be reloaded without restart
7. ⏳ System can recompile itself
8. ⏳ Basic PEG parser support
9. ⏳ Simple network REPL

**Current Progress: ~45% complete**

---

## Long-term Vision

Once MVP is solid, explore:

- **Incremental compilation**: Only recompile changed functions
- **Time-travel debugging**: Snapshot before each change, allow rollback
- **Visual programming**: GraphQL-like graph of objects/messages
- **Distributed objects**: Objects on different machines
- **Capability-based security**: Objects as capabilities
- **Persistent processes**: System survives host restarts

---

## Notes & Thoughts

**On "Smalltalk but with PEG":**
- Smalltalk's syntax is fixed; we want programmable syntax
- PEG parsers let users define their own syntax
- Grammar becomes part of the program, stored as objects
- Can have multiple syntaxes in same image

**On "Image-based + File-based":**
- Smalltalk's pure image approach loses version control benefits
- Pure file-based loses live programming benefits
- Birnam aims for best of both: primary source in files, mutable in memory
- Image snapshots for quick save/restore, but files are canonical

**On "Hot reloading everything":**
- Unlike most systems, even the object system itself should be reloadable
- This requires careful ordering and state management
- Start with modules, work up to system, eventually to libid itself

**On "Network accessibility":**
- nREPL revolutionized Clojure development (editor integration, remote debugging)
- Birnam should be network-first from the start
- Enables tools that are separate processes (better isolation, language agnostic)

---

## Development Workflow

**Current workflow:**
```bash
# 1. Edit code
vim src/sys/system.c

# 2. Build
make system

# 3. Test
./bin/system
>> help
>> exit

# 4. Repeat
```

**Future workflow (with hot reloading):**
```bash
# 1. Start system once
./bin/system

# In REPL:
>> edit: "System"           # Opens editor on System module source
>> # Make changes in editor, save
>> System reload            # Recompiles and reloads System
>> # Changes take effect immediately
```

---

## References

- **Ian Piumarta's COLA**: http://piumarta.com/software/cola/
- **IDST/Pepsi documentation**: `~/dev/c/idst-with-history/doc/pepsi.html`
- **Original libid**: `~/dev/c/idst-with-history/object/id/`
- **This repo**: `~/dev/c/birnam/`

---

*Last updated: 2025-10-29*
*Current phase: Core Infrastructure (Phase 0→1 transition)*
