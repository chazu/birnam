# 81U — Boolean methods above Id

> **Status:** Implemented, following Word representation in 81T.

Move Boolean's twelve existing method implementations and their initialization
from Prelude into `birnam-boolean`. The nine base methods and three False branch
overrides use the existing Method/module-init interfaces. The module depends
only on the ABI crate and object-model's ordinary private-behavior protocol.
Load it immediately before Prelude, after object-model, to retain its existing
position relative to the compiler rungs. Pin its code for installed methods.

Keep every method body unchanged. Conditional branch methods deliberately do not
check receiver identity; inherited branch methods retain their current behavior.
The eager and lazy operators, `not` and `printString` do check singleton identity.
Preserve arbitrary right operands, short-circuiting, fresh `value` lookup, nil
results for invalid arity/arguments, current Id callbacks and String creation.
False keeps its private behavior delegating to Boolean. Reinitialization uses the
current behaviors and reinstalls methods; later Prelude leaves those methods
alone. Do not introduce a Boolean operations table or optimizer declaration.

The true/false singleton objects, nil, their roots and IdApi fields remain in Id.
This is a method extraction, not a singleton-representation migration. IdApi v6,
Method, Frame and image formats remain unchanged. The shared singleton fields
and String-owned buffers require separate caller, startup and lifetime audits.

Review the move for hidden dependencies and indirect calls: no compiler or Error
is required to execute these methods; only branch objects' ordinary `value`
protocol is used. The module owns no new GC roots or payload. Preserve the
private-behavior setup before installing either set of branch methods, including
its failure checks. Relocate the source bodies mechanically and compare them to
the committed Prelude; do not normalize away differences in an independent
oracle.

Validate with an independent C truth/branch oracle, explicit evaluation counts,
invalid operands, inherited behavior, replacement, reinitialization, live native
callbacks and collection. Verify debug startup, a source-free compiled module,
a saved image containing native Code and a baked executable before committing.
No release validation or new functionality.

## Validation

The independent C fixture passes against both the prior Prelude and the new
Boolean module: 34 truth/branch cases plus evaluation counts, arbitrary eager
operands, inherited branch receivers, exact singleton recognition for the other
operators, malformed arguments, fresh branch lookup, live Id send/String
callbacks, reinitialization, namespace rebinding and GC during branch execution.
The new module also preserves a replacement through later Prelude initialization.
All sixteen method/helper bodies and the installation block match the previous
Prelude byte-for-byte.

Host tests pass 86 with one ignored; its builder passes four and Prelude passes
two. Boolean Clippy passes with warnings denied. The development and installed
dylibs match the two declared exports and relocatable install name, and link
only the system library. IdApi, Id representation and Image sources are unchanged.

A fresh external debug prefix contains 23 native libraries and 28 compiled
Birnam modules. All 1,469 source hashes remain unchanged during installation.
The installed native fixture, startup, a compiled module after source removal,
a saved v8 image containing native Code and a baked executable pass. Both image
modes exercise branch selection, short-circuiting, captured singleton identity,
method replacement and collection, and confirm dynamic Boolean loading.

Run `mise run boolean` and `scripts/boolean-image-smoke.sh PREFIX`. Local logs are
`.cargo-target/81u-{check,baseline,native,unit,clippy,prefix,prefix-native,image}.log`.
No release validation ran.

## Next candidate

Nil's five existing methods in Collection are another small independent value
protocol. Move those before tackling String's methods, which also couple to
Array construction. Keep nil's identity and roots in Id for that checkpoint;
removing shared singleton fields needs a separate boot/caller audit. String's
external byte ownership and finalization remain their own representation task.
