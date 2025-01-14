IMPORTANT

- If you wish to dynamically link libtcc, make sure you are configuring tinycc to build a dynamic object via ./configure --disable-static



[] Take first pass at defining the module interface
[] Rework obj.c so it follows the module interface
[] Move repl out of main

[] Load obj.c as a module into system - without using send
[] in system, expose module api in terms of object system
[] rewrite system to follow module interface
[] once object system is pulled into system, reinitialize system in terms of object system
--------------------

NOTE see bottom of file for latest note about unfucking things.

_A_ system is a binary object that can be run as an executable OR loaded dynamically, and which exposes a complete - and completely adaptive - system.

# TODO
## Phase -2 - What have I done?!
   [x] DONE System loads obj and calls begin
   [x] System can get the contents of the symbol table(s) when loading obj
   [ ] System calls into obj to initialize its contents into the system, if applicable - see Eli Benderskis C plugin articles
   [ ] System can access objects and call _methods_ inside of obj at runtime, via repl. How do we do this? Some standard contract between system and module
### Files we'll create
  - sys/repl.c for repl functionality
  - sys/system.c or main.c for main
  - sys/obj.c for object model
  - sys/peg for parser generation - thin OO wrapper around packrat
  - sys/compiler.c for compilation - embeds tcc.
### Thoughts
  - system core (main) loads these modules in at runtime.
  - what do we call a compiled artifact? A module
  - a "c module" is a module which the system knows how to use via C
  - a "wrapped module" is a c module which the system can load at runtime, calling into it dynamically with message passing. We'd like to be able to interactively and exploratorily wrap existing libraries in this way. That is, load a dynamic object, poke around, write tests, wrap functions in message passing API, write out the FFI code. REPL-based FFI code.
  - a "native module" is a module which is written natively in birnam.
### CLEANUP FROM PHASE -2
    [ ] Currently we're using a custom build of libelf from vendor, transplanted into /usr/lib. Lets make sure this works with stock libelf dynamically OR lets statically bundle libelf into the elf module.

## Phase -1 - WTF Am I Doing?
 - system is an executable that loads the core of the system into a harness. therefore
   - it loads obj
 - system can accept commands:
   - compile <file> - compiles file into a.out or whatever
     - cant find crt01.o on compilation of helloworld.c - figure that out.
   - repl <file> - loads compiled file into system and starts repl - can send messages to call functions
## Phase -0.5
 - Compile command takes --system and compiles the target as a system, meaning it can compile and repl.
## Phase 0 - Able to load and call functions and observe their side effects in system
 - restructure repo to be more modular - like one of our normal C/C++ repos
 - experiment: compare trivial binaries with and without various tcc options like call stack traces, debug info, etc
 - whip up an example of listing the symbols in a dynamic lib
 - Can we use thunks or thunking to help make exploration of alien libraries easier?
 - try making toy have a main function and see if you can call it
 - how do we implement something like lua's stack so we can just push args and call an alien function ?
 - bootstrap system with super-basic libid implementation (dynamic dispatch to commands)
 - load libtecla into system on startup
 - provide tab-completion for commands and files in system
## Phase 1 - Able to capture return values of symbols for re-use in system's REPL
## Phase 2 - Able to edit and compile a grammar into an executable/library in system
## Phase 3 - Able to load and unload grammars in the REPL, and activate/deactivate them
## Phase 4 - Able to create, compile and load a grammar using a grammar built with system

# DONE
vendor libtecla

# Bootstrapping Birnam
- Compile libtcc as a dynamic library
- Compile system

# No really, bootstrapping it
- Seems like u need dynload - which iirc is a pain to compile into a dynamic library
- i think i need to re-jigger this whole thing so that the object system "bootstraps" itself (via libtcc, so not really bootstrapping) without needing _any_ external deps beyond libtcc. in other words, reimplement elf.c without using dynload.
