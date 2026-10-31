# Birnam Language Design

## Inspiration and Vision

Birnam is a thoroughly late-bound, AOT-compiled, prototype-based programming language inspired by Ian Piumarta's idst, Self and Smalltalk. It aims to fullfil VPRI's vision of a small, comprehensible and completely malleable programming environment.

### Invariants

Birnam aims to meet the following criteria:
- Comprehensible - its implementation must be as small as possible, and ideally comprehensible with a minimum of effort.
- Malleable - A user (what we shall call the programmer) should be able to replace any part of the system at any time. Changes should be able to be loaded into the system at any time, live. NOTE - no guarantees are made that the system will not be broken by changes the user makes.
- Performant - We aim to approach the speed of C execution wherever possible, while maintaining a debuggable and live environment. To do this, we will leverage deoptimization techniques as seen in the Self language, and aim to make practical birnam code as simple while remaining expressive.

## Pragmatics

At it's lowest level, Birnam uses the id object system (see Piumarta) as it's lowest level, fully malleable core. Above or alongside this is layered a stackless concurrent runtime and a small but practical standard library.

Birnam is written initially in Odin, and includes a modified version of libtcc which allows compilation of relocatable C directly into the memory of the current process. Birnam code is lowered and converted into C which is then compiled and can be saved to disk as a complete executable or loaded back into the running Birnam program.

Birnam can compile and run code in different ways:
* Siphon mode: The siphon is a minimal program which knows only the shape of birnam's id API (implemented first in the siphon, then in a birnam module) and how to load and reload birnam modules.
* Birnam Modules: individual dynamically-linked artifacts which birnam can load and which speaks the id message-passing API.
* Complete Image: An executable and dynamically loadable binary which also serves as an 'image' - at startup, each portion of the birnam stack is loaded from the siphon through to the id layer and then the rest of the base system, plus any additional modules compiled into the image. The image can be modified at runtime, saved back to disk, and reloaded. Additional compiled birnam modules can be dynamically loaded into the image, including application level code. Source code can be filed-in to the image and compiled.
* Baked image: A birnam image can take a subset of code and 'bake' it into a standalone executable which removes all non-essential application code - including the siphon and programming utilities, metadata about the code such as source code, et cetera. This is how applications can be distributed to end users.

## Startup

Siphon -> loads id module, replacing its own id implementation
-> loads additional modules
