# System GC controls v1

Design [81AJ](../docs/design/81aj-production-system-gc.md) moves System gc,
liveObjects, autoGc: and preciseGc: into `birnam-system-gc`. Load it immediately
before Prelude, attaching Integer v1 before initialization. Initialization only
enables `birnam_system_gc_provider_v1`; it creates no objects, roots, layouts or
methods. Prelude retains System creation/adoption and all twelve selector interns.

Attach the immutable, process-pinned descriptor through
`birnam_system_gc_dependency_v1`. It is 24 bytes on arm64: version 1 at offset 0,
byte size at 8, and one install pointer at 16. The
[independent C mirror](fixtures/system-gc-v1/system_gc_v1.h) checks the layout.
Admission rejects null, wrong version/size, missing install and retargeting;
repeat attachment of the same pointer is accepted.

The operation is `install(registration, execution, behavior, gc, liveObjects,
autoGc, preciseGc)`. Both tables are const IdApi pointers; the remaining arguments
are Oops. Prelude calls it at the original final four registration slots after
readLine, supplying the already-selected behavior and selector identities. The
installer never reinterns or retains these Oops. It registers methods in order
using the current registration table's live method callback. It retains the first
supplied execution pointer, which must be Prelude's original selected table,
even if initialization reentered through another table before first installation.
Tables and module code remain process-owned and pinned; calls run on the owner
thread. Reinstallation preserves execution while using current registration data.

The two zero-argument queries ignore receiver/carrier, call the current collector
callback, cast its usize answer to i64, and then select Integer's current make
operation/context. The query may change that constructor; null and alias make
results pass through unchanged. Nonzero arity returns current nil with no query.

The one-argument setters reject malformed carriers and values unequal to the
current true/false identities. Exact identity is authoritative, including null or
aliased identities, with true taking precedence. Valid calls forward 0/1 to the
current callback and return the original receiver, including null. Invalid calls
return current nil without changing collector state. No exact receiver-type check
or error construction is added.

The collector implementation, System representation, IdApi v7 and image encoding
are unchanged. Run `mise run system-gc` and `scripts/system-gc-image-smoke.sh PREFIX`
for focused debug native checks, startup, a source-free compiled module, saved
v8 native Code image and baked executable.
