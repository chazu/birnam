# 81V — Nil methods above Id

> **Status:** Implemented, following Boolean methods in 81U.

Move five existing methods from Collection into `birnam-nil`: Object's `isNil`
and `notNil`, and nil's overrides plus `printString`. Load the module immediately
before Collection, after Prelude. It depends only on the ABI crate and uses
existing IdApi/Method/module-init interfaces. Keep method bodies unchanged and
pin native code for all installed methods.

These methods deliberately answer according to the selected method, not a
receiver-identity test: inherited nil behavior and direct native method calls
retain their current answers. Nonzero arity answers nil. Printing allocates a
fresh String through the current callback. Initialization uses the current nil
behavior; repeated calls reinstall methods without creating singleton objects.
Later Collection initialization must preserve replacements.

Nil's canonical identity, behavior and roots stay in Id, as do the Boolean
identities used for answers. No operations table, optimizer declaration, new
functionality or format change is introduced. IdApi v6 remains 37 fields. The
shared singleton fields need a separate caller/first-boot audit before removal.

Review this boundary for Collection coupling: the five methods use no Collection
state, numeric operations, Array construction, or native buffer ownership. Keep
the method bodies and selector/target choices identical to the committed source.
Verify an independent C oracle against the prior Collection and the new module,
including inherited behavior, invalid arity, live String construction, repeated
initialization, replacement before/after Collection, and GC. Finish with debug
startup, a source-free compiled module, a saved native-code image and a baked
executable, then commit. No release validation.

## Validation

The independent C fixture passes against both the prior Collection and the new
nil module: 30 method/receiver combinations, inherited behavior, invalid arity,
fresh Strings, live String construction and Boolean identity fields, namespace
rebinding, reinitialization and GC. The new module also preserves Object/nil
method replacements through later Collection initialization. All five method
bodies and the installation block match the prior Collection byte-for-byte.

Collection's four tests pass, as do 86 host tests (one ignored) and four builder
tests. Nil Clippy passes with warnings denied. Development and installed dylibs
match the two declared exports and relocatable install name, linking only the
system library. IdApi, Id representation and Image sources are unchanged.

A fresh external debug prefix contains 24 native libraries and 28 compiled
Birnam modules. All 1,477 source hashes remain unchanged during installation.
The installed native fixture, startup, a compiled module after source removal,
a saved v8 image containing native Code and a baked executable pass. Both image
modes exercise Object/nil predicates, printing, captured singleton identity,
live method replacement and collection, and confirm dynamic nil loading.

Run `mise run nil` and `scripts/nil-image-smoke.sh PREFIX`. Local logs are
`.cargo-target/81v-{check,baseline,native,unit,clippy,prefix,prefix-native,image}.log`.
No release validation ran.

## Next candidate

String has fourteen methods that can move independently of Collection's storage:
printing, concatenation, equality, byte/index/slice operations, trimming, search,
bracket depth, replacement and Symbol/number conversions. They need the current
String primitives, explicit Integer/Float dependencies and a retained printing
selector. Review those method bodies and shared helpers before moving them.

Keep `split:` with Collection for now: it calls native Array construction and
push operations directly, so switching it to ordinary Array sends would change
its replacement behavior. Collection's parent association and StringBuilder
also need distinct checkpoints. String representation still owns external byte
buffers and has finalizer/optimizer observation obligations in Id; do not treat
that migration as another raw eight-byte payload move.
