# Printing defaults v1

Design [81AI](../docs/design/81ai-production-printing-methods.md) moves Object
printString/asString and Symbol printString from Prelude into `birnam-printing`.
Design 83 loads Printing before collections and parser/compiler modules, with
completed String operations supplied before initialization. Its `module_init`
installs the three defaults immediately: two selector interns, then the three
registrations. It creates no objects, roots or layouts. Prelude no longer
installs printing or attaches its installer.

Attach the immutable, process-pinned descriptor through
`birnam_printing_dependency_v1`. It is 24 bytes on arm64: version 1 at offset 0,
byte size at 8, and one function pointer at 16. The
[independent C mirror](fixtures/printing-v1/printing_v1.h) checks that layout.
Admission rejects null, wrong version/size, a missing operation and retargeting;
repeated attachment of the same descriptor pointer is accepted.

`install(IdApi *registration, const IdApi *execution)` uses the current table
for interning and registration targets. Execution is the original selected
table. The provider retains that execution pointer before any interning callback
can reenter initialization; reinstallations do not replace it. Both tables and module
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
identities, the current IdApi, layouts and image encoding are unchanged. Run
`scripts/id-native-loading-smoke.py PREFIX` for the installed value prefix and
`scripts/printing-image-smoke.sh PREFIX` for startup, source-free compiled, saved
image and baked execution. The older `mise run printing` driver still assumes
aggregate `id_init` construction and Prelude installation; it requires migration.
