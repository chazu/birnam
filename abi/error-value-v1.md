# Lazy Error value policy v1

Design [81AM](../docs/design/81am-production-error-value.md) moves Prelude's
lazy Error construction and exact-recognition helpers into `birnam-error-value`.
All existing Prelude, Integer and Float consumers, plus Word's asInteger overflow
path, use the dependency. Error representation and public methods stay separate.

Load the provider immediately before Integer activation, attaching prepared String
v1 first. String/Word/Float storage preparation retains its order before both.
Initialization enables the provider without creating objects, methods, roots or
layouts. Attach the immutable, process-pinned descriptor through
`birnam_error_value_dependency_v1`. Its arm64 layout is 32 bytes: version 1 at 0,
byte size at 8, is_error at 16 and from_bytes at 24. Admission rejects null, wrong
version/extent, either missing operation and retargeting; repeat attachment of the
same pointer succeeds. The [independent C mirror](fixtures/error-value-v1/error_value_v1.h)
checks this layout.

Both operations receive the caller's live selected IdApi each time. Recognition
returns a C int (0/1), first skipping all callbacks for null. It checks ErrorVt
export presence, imports the current behavior and, if nonnull, compares that
snapshot with the subsequent vtableOf result. It performs no isError send or
inherited-behavior recognition.

Construction receives kind/message pointers and lengths. They are valid nonnull
borrowed slice pointers, unchanged throughout the call, including for empty
extents; the kind includes its NUL
terminator as required by intern. Reconstruct the slices without scanning or
copying. Intern the kind, make the message with String's live constructor, then
check/import Error. These first two callbacks occur even if Error is absent.
An absent/null factory returns current nil. Otherwise intern kind:message:
freshly and ordinarily send the two resulting Oops to the imported factory
snapshot. Null/alias arguments and arbitrary send results pass through unchanged.

The provider retains no runtime table, Oops, selectors, roots or byte buffers.
Callers preserve the existing owner-thread callback and borrow contracts;
callbacks may reenter and collect. No API extends any value or storage lifetime.
IdApi v7, Error payload, image encoding and language behavior are unchanged.
Run `mise run error-value` and `scripts/error-value-image-smoke.sh PREFIX` for
focused debug consumer checks and startup/source-free compiled/saved-image/baked
execution.
