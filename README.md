
_A_ system is a binary object that can be run as an executable OR loaded dynamically, and which exposes a complete - and completely adaptive - system.

# TODO
## Phase -2 - What have I done?!
   - System loads obj and calls begin DONE
   - System gets the contents of the symbol table when loading obj
   - System calls into obj to initialize its contents into the system, if applicable
   - System can access objects and call _methods_ inside of obj at runtime, via repl
     How do we do this?
## Phase -1 - WTF Am I Doing?
 - system is an executable that loads the core of the system into a harness. therefore
   - it loads obj
 - system can accept commands:
   - compile <file> - compiles file into a.out or whatever
     - cant find crt01.o on compilation of helloworld.c - figure that out.
   - repl <file> - loads compiled file into system and starts repl - can send messages to call functions
## Phase -0.5
 - Compile command takes --siphon and compiles the target as a system, meaning it can compile and repl.
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
## Phase 1 - Able to capture return values of symbols for re-use in siphon's REPL
## Phase 2 - Able to edit and compile a grammar into an executable/library in system
## Phase 3 - Able to load and unload grammars in the REPL, and activate/deactivate them
## Phase 4 - Able to create, compile and load a grammar using a grammar built with system

# DONE
vendor libtecla

# Bootstrapping Birnam
- Compile libtcc as a dynamic library
- Compile system
