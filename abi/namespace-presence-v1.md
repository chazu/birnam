# Borrowed Namespace presence v1

Design [81BA](../docs/design/81ba-production-error-presence-handoff.md) introduces
the explicit per-call presence handoff used by [Error Value v2](error-value-v2.md).
The [C mirror](fixtures/namespace-presence-v1/namespace_presence_v1.h) describes
the 32-byte arm64 descriptor: version 1 at 0, extent at 8, opaque context at 16,
and `contains(context, c_name) -> int` at 24.

This is a borrowed capability, not an owned provider or process-global dependency.
The caller guarantees a valid readable descriptor, selected code, context and
NUL-terminated name for each call. Context may be null if the callback allows it.
Access is owner-thread serialized. The recipient snapshots function and context
together immediately before invocation and retains no reference across callbacks.
Callbacks may reenter, collect and replace the descriptor for subsequent calls.
Zero means absent; every nonzero result, including negative values, means present.

Current call-site adapters borrow the address of the caller's live function slot.
The slot bridge reads that function only when presence is checked, so changes
during earlier callbacks remain visible. It neither knows nor recovers an IdApi,
retains the slot beyond the call, nor copies the callback at handoff. This bridge
is a migration adapter; IdApi field removal and native initialization transport
remain separate work. No ownership, root or object lifetime is extended.
