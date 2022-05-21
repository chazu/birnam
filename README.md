Note 5/2022 - I think we need to nix the idea of the siphon - in favor of some idea like compiling something as "whole" or some other clever-but-not-too-clever bullshit like that, meaning it contains the whole birnam toolchain and can compile both plain executables and new "whole" executables.

_A_ siphon is a binary object that can be run as an executable OR loaded dynamically, and which exposes a complete - and completely adaptive - system. Our prototype siphon is called siphon.

# TODO
## Phase -1 - WTF Am I Doing?
 - siphon can accept commands:
   - compile <file> - compiles file into a.out or whatever
   - repl <file> - loads compiled file into siphon and starts repl - can send messages to call functions
## Phase -0.5
 - Compile command takes --siphon and compiles the target as a siphon, meaning it can compile and repl.
## Phase 0 - Able to load and call functions and observe their side effects in siphon
 - restructure repo to be more modular - like one of our normal C/C++ repos
 - experiment: compare trivial binaries with and without various tcc options like call stack traces, debug info, etc
 - whip up an example of listing the symbols in a dynamic lib
 - Can we use thunks or thunking to help make exploration of alien libraries easier?
 - try making toy have a main function and see if you can call it
 - how do we implement something like lua's stack so we can just push args and call an alien function ?
 - bootstrap siphon with super-basic libid implementation (dynamic dispatch to commands)
 - load libtecla into siphon on startup
 - provide tab-completion for commands and files in siphon
## Phase 1 - Able to capture return values of symbols for re-use in siphon's REPL
## Phase 2 - Able to edit and compile a grammar into an executable/library in siphon
## Phase 3 - Able to load and unload grammars in the REPL, and activate/deactivate them
## Phase 4 - Able to create, compile and load a grammar using a grammar built with siphon

# DONE
vendor libtecla

# Bootstrapping Birnam
- Compile libtcc as a dynamic library
- Compile siphon
