# Printing defaults v1

Design [81AI](../docs/design/81ai-production-printing-methods.md) moves Object
printString/asString and Symbol printString from Prelude into `birnam-printing`.
Load it immediately before Prelude, attaching the prepared String v1 dependency
before initialization. Initialization only enables `birnam_printing_provider_v1`;
it creates no objects, roots, layouts or methods. Prelude invokes installation
before its Block and System setup, preserving the two selector interns followed
by the original three registrations.

Attach the immutable, process-pinned descriptor through
`birnam_printing_dependency_v1`. It is 24 bytes on arm64: version 1 at offset 0,
byte size at 8, and one function pointer at 16. The
[independent C mirror](fixtures/printing-v1/printing_v1.h) checks that layout.
Admission rejects null, wrong version/size, a missing operation and retargeting;
repeated attachment of the same descriptor pointer is accepted.

`install(IdApi *registration, const IdApi *execution)` uses the current table
for interning and registration targets. Execution is Prelude's original selected
table. The provider retains that execution pointer before any interning callback
can reenter Prelude; reinstallations do not replace it. Both tables and module
code must stay process-owned and pinned. Calls and installation run on the
runtime owner thread. Methods read live operations and identity fields rather
than copying table contents or retaining selectors.

Object printString accepts any receiver and constructs a fresh anObject String
at zero arity. Object asString interns printString and performs an ordinary send
each time, retaining custom lookup effects and returning its result unchanged.
Symbol printString accepts whatever the live symbolName callback recognizes,
returns nil for a null name, and copies the NUL-terminated bytes with a leading #
before String construction can allocate or reenter. It performs no UTF8 conversion
or escaping. Null/alias String make results are returned unchanged. All three
methods return the retained table's current nil for nonzero arity.

No forwarding Method remains in Prelude. Symbol interning/storage, singleton
identities, IdApi v7, layouts and image encoding are unchanged. Run
`mise run printing` and `scripts/printing-image-smoke.sh PREFIX` for focused debug
native checks, startup, a source-free compiled module, saved v8 native Code image
and baked executable.
