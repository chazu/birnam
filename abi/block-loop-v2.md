# Block Loop installation v2

Design [81AT](../docs/design/81at-production-block-loop.md) moves native Block
whileTrue:, whileFalse: and their common loop body intact from Prelude into
`birnam-block-loop`. Its sole dependency is Error Value v2; it loads before
Prelude. Initialization enables its provider without creating Id objects,
methods, roots, capture layouts or scheduling state.

Attach the immutable process-pinned descriptor with
`birnam_block_loop_dependency_v2`. Its arm64 layout is 24 bytes: version 2 at 0,
byte size at 8 and install at 16. Admission rejects null, wrong version/extent,
missing install and retargeting; the same pointer may attach again. The
independent C mirror (`abi/fixtures/block-loop-v2/block_loop_v2.h` at `eb219997`) checks this layout.

Install receives const registration/execution IdApi pointers, an execution presence pointer, the selected Block
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
pinned. Public selectors and image encoding are unchanged. The
[export inventory](rust-block-loop-dylib-v1.json) declares four symbols.

Run `mise run block-loop` for native checks. The focused
`scripts/block-loop-image-smoke.sh PREFIX` reloads Prelude in startup, compiled
module and saved/baked image programs to exercise the native implementations
explicitly. An independent C guard verifies both Method addresses belong to
Block Loop. The normal Block suite separately covers authored stackless startup.

Design [81BB](../docs/design/81bb-production-namespace-presence.md) versions this
installer to v2. Its install function receives the execution API's explicit
`const NamespacePresenceV1 *` immediately after the execution API pointer.
Capture the pair together at the original first-install point, before callbacks;
later registration does not replace either execution selection. The caller pins
the live descriptor, its context and code for the retained method lifetime.
Each presence check selects the current function/context at invocation.
