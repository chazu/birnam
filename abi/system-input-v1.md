# System input installation v1

Design [81AQ](../docs/design/81aq-production-system-input.md) moves System readLine
intact from Prelude into `birnam-system-input`. It depends on prepared String v1
and Error Value v1 and loads immediately before Prelude. Initialization enables
the provider without creating objects, methods, roots, layouts or input buffers.

Attach the immutable, process-pinned descriptor through
`birnam_system_input_dependency_v1`. Its arm64 layout is 24 bytes: version 1 at
0, byte size at 8 and one install function at 16. Admission rejects null, wrong
version/extent, missing install and retargeting; repeat attachment of the same
pointer succeeds. The [independent C mirror](fixtures/system-input-v1/system_input_v1.h)
checks layout without Rust headers.

Install receives two const IdApi pointers, the selected System behavior and the
already interned readLine selector. Prelude keeps System creation/adoption, all
twelve selector interns and the original registration slot, after loadModule:
and before GC controls. Use the current registration table for the method
callback. Retain Prelude's original execution pointer before that callback can
invoke the method or reenter; this can differ from the first registration table.
Later installations keep the original execution selection. Use the supplied
behavior and selector synchronously without reinterning or retaining them.

The method ignores receiver and argument pointer. Nonzero arity returns current
nil without reading. A zero-argument call locks stdin and reads through LF or
EOF, removes only a final LF and passes the remaining raw bytes to the current
String constructor/context. Empty lines produce empty Strings, while immediate
EOF returns current nil. CR, NUL and invalid UTF-8 bytes remain unchanged. The
existing String constructor's null or alias result is returned unchanged.
Read failure uses the existing lazy ioError policy and "stdin read failed"
message through Error Value with the retained execution table.

The stdin lock and line buffer remain live through String/Error callbacks.
Recursive readLine from such a callback blocks on the same lock in the baseline
implementation; this extraction preserves that behavior. Independent child
process checks bound the observation without hanging the test runner. Ordinary
successive calls retain buffered input as before. IO state is not added to the
saved object graph, and no normalization or new collection point is introduced.

The native method and descriptor remain pinned. System identity, IdApi v7,
image encoding, public selector and fresh lookup are unchanged. The
[export inventory](rust-system-input-dylib-v1.json) declares the five symbols.
Run `mise run system-input` and `scripts/system-input-image-smoke.sh PREFIX` for
focused debug checks and startup/source-free compiled/saved-image/baked execution.
