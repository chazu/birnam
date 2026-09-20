# Source Loader installation v2

Design [81AV](../docs/design/81av-production-source-loader.md) moves System load:
intact from Prelude into `birnam-source-loader`. It depends on Integer, String,
Text View and Error Value; Prelude drops their now-unused attachments. Source
Loader loads before Prelude and initializes only its provider: no Id objects,
methods, roots or compiler state.

Attach its immutable process-pinned descriptor through
`birnam_source_loader_dependency_v2`. The arm64 layout is 24 bytes: version 2 at
0, byte size at 8 and install at 16. Null, wrong version/extent, missing install
and retargeting are rejected; the same pointer may attach again. The
[independent C mirror](fixtures/source-loader-v2/source_loader_v2.h) checks layout.

Install receives const registration/execution IdApi pointers, an execution presence pointer, the selected System
behavior and already interned load: selector. Prelude retains System/Block
identity, layout, creation/adoption, all selector interns and the original
registration order. Retain its first execution pointer before method callbacks,
including reentry with another registration table. Reinstallation keeps that
pointer. Use behavior/selector synchronously without retaining or reinterning.

Wrong arity, null carriers and unavailable exact String paths return current
nil. Invalid UTF-8 paths or file failures use the existing ioError. Read at most
one GiB plus one byte to reject excess length before checking Codegen. Keep the
source bytes owned through callbacks and the path borrowed through diagnostics.
After selecting Codegen through Id's export table, use the current String
constructor, then send sourceRevisionOf:. For a non-Error revision unequal to
current nil, send at: with the current Integer constructor's result for 1, then
recordModuleSource:named:; only an exact Error recording result stops evaluation.

Send evalPersisting:from: with the source object and original path object. A
result equal to current nil triggers lastError. A non-nil lastError produces the
existing syntaxError, printing an exact String diagnostic when available. Other
exact Error results use printable diagnostics and return unchanged. A non-Error
result may send markLoaded: to Module when its selected name differs from nil
and Module is present/non-null in Id's export table; ignore that notification's
result. All sends, recognition, snapshots and callback ordering are unchanged.

In the checked production baseline, the authored Module lives in Globals but is
absent from Id's export table, so this optional notification is skipped. A source
declaration can be registered while isLoaded: remains false. This extraction
preserves that existing distinction; it does not bridge the two registries.

The method, descriptor and original execution table remain pinned. No public
protocol, object representation or image format changes. The
[export inventory](rust-source-loader-dylib-v1.json) has seven symbols. Run
`mise run source-loader` and `scripts/source-loader-image-smoke.sh PREFIX` for
focused debug checks. The image fixture removes caller source while retaining
separate input source files that load: deliberately reads and evaluates.

Design [81BB](../docs/design/81bb-production-namespace-presence.md) versions this
installer to v2. Its install function receives the execution API's explicit
`const NamespacePresenceV1 *` immediately after the execution API pointer.
Capture the pair together at the original first-install point, before callbacks;
later registration does not replace either execution selection. The caller pins
the live descriptor, its context and code for the retained method lifetime.
Each presence check selects the current function/context at invocation.
