# Block Loop installation v1

Design [81AT](../docs/design/81at-production-block-loop.md) moves native Block
whileTrue:, whileFalse: and their common loop body intact from Prelude into
`birnam-block-loop`. Its sole dependency is Error Value v1; it loads before
Prelude. Initialization enables its provider without creating Id objects,
methods, roots, capture layouts or scheduling state.

Attach the immutable process-pinned descriptor with
`birnam_block_loop_dependency_v1`. Its arm64 layout is 24 bytes: version 1 at 0,
byte size at 8 and install at 16. Admission rejects null, wrong version/extent,
missing install and retargeting; the same pointer may attach again. The
independent C mirror (`abi/fixtures/block-loop-v1/block_loop_v1.h` at `eb219997`) checks this layout.

Install receives const registration/execution IdApi pointers, the selected Block
behavior and already interned whileTrue:/whileFalse: selectors. Prelude retains
Block creation/adoption, identity, capture layout and original registration
positions. Retain its first execution pointer before either method callback,
even when registration reenters with another table. Reinstallation keeps this
pointer. Use behavior/selectors synchronously without retaining or reinterning.

The methods accept one body argument, copied before the first callback. Wrong
arity or a null carrier returns current nil. Each iteration freshly interns and
sends value to the condition receiver using ordinary dispatch, then checks exact
Error recognition before comparing against the current true and false identities.
A non-Boolean condition uses the existing lazy mustBeBoolean error and message.
A terminating condition returns current nil without evaluating the body. Otherwise
a fresh value send evaluates the body and exact Error results propagate; any
other result is ignored before the next iteration. Receiver/body types are not
prechecked. All callback order, live table reads, imported Error snapshots and
null/alias results remain unchanged.

These are the bootstrap native loops. Full startup later loads
`stackless-control.birnam`, which installs authored methods on the same Block
family. This extraction preserves that replacement and adds no scheduler or
stackless implementation. Methods, descriptor and original execution table remain
pinned. IdApi v7, public selectors and image encoding are unchanged. The
[export inventory](rust-block-loop-dylib-v1.json) declares four symbols.

Run `mise run block-loop` for native checks. The focused
`scripts/block-loop-image-smoke.sh PREFIX` reloads Prelude in startup, compiled
module and saved/baked image programs to exercise the native implementations
explicitly. An independent C guard verifies both Method addresses belong to
Block Loop. The normal Block suite separately covers authored stackless startup.
