# Birnam Design Document

To describe clearly and concisely the design of the birnam language

## Vision

Birnam is a completely live, completely malleable programming environment in the spirit of Piumarta's COLA project. It is, effectively, intended to be an implementation of that vision, albeit a naive one.

## Design Criteria

### Easily bootstrappable

Birnam, unlike Smalltalk-80 or Piumarta's reference implementation of COLA, should continue to be fully bootstrappable after it is stable and mature (for some value of the word mature). No piece of the chain required to bootstrap the codebase from its initial form into one which is self-perpetuating should be discarded.

### Fully malleable

Any aspect of the system should be modifiable at both run-time and compile time.

### Fully live environment

Birnam should support a live, 'image-based' style of development in which stepwise mutation of the system (or the application built in the system) is carried out by the programmer/user. At the same time, Birnam should treat external, file-based representations of code as first-class citizens and allow users to store, version, and iterate on their code as files. Tools should be provided to decrease or eliminate the impedence mismatch between these modes of working.

### Malleable system but can still ship code

While mutating the 'system' portions of Birnam should be considered a core feature of the language, the ability of users to build, version, and ship applications independent of this system portion should be a priority.


## Implementation

### Terms

#### System

A 'system' refers to a birnam executable which contains the birnam language and environment itself. This is in opposition to the term 'application', which refers to an executable (or dynamically or statically loadable) artifact which is created _by_ the system but which no longer contains the core portions of the Birnam environment which make it live and fully malleable.

A system is both a dynamically linked object and an executable object.

#### Module

A module is code loaded by birnam. This can take the form of dynamically loadable object or one or more source files.


### Bootstrapping

The current prototypal design of Birnam is pseudo-bootstrapped thusly:

* libtcc is compiled as a dynamic library using tcc
* The object module (obj.c) is compiled into a dynamic library using tcc
* The main entrypoint (system.c) is compiled as a system - that is, as a dynamically loadable object and as an executable
* On running, system imports the object module, initializing the object model
* The system then registers its own API in terms of the object system
  - What does this mean?
* Depending on the way the system was invoked, it either launches a repl, compiles files, or executes some entrypoint from a module as specified on the command line

### Core modules
- Object system
- Compiler (wrapper around libtcc)
- parser generator (wrapper around packcc or peg)
- Module system
- repl?

### Module interface
- How do we load a module without colliding symbols? Each module must have some metadata and its symbols must be uniquely named but according to some convention that the system is able to use to pull data into the object system
- Perhaps there is a well-known symbol that returns the metadata for a module
- This makes hand-writing a module in C fairly simple, right? The module itself wouldn't contain its own hash, thats impossible - it will just contain a placeholder which the module loader will replace with the hash of the module's data.
