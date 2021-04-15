_A_ siphon is a binary object that can be run as an executable OR loaded dynamically, and which exposes a complete - and completely adaptive - system. Our prototype siphon is called siphon.

# TODO

## Phase 0 - Able to load and call functions and observe their side effects in siphon
 - restructure repo to be more modular
 - whip up an example of listing the symbols in a dynamic lib
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
