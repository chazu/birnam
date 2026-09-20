# 81BC — Namespace owns interned publication

> **Status:** Implemented and debug-validated, following presence in 81BB.

Move the existing intern/bind/mirror sequence from `Runtime::export` into the
loaded Namespace module. The selected restricted interner runs once. A rejected
name stops without binding, mirroring or writing the result. Otherwise invoke the
selected raw bind, update Id's weak environment dictionary with the original
Symbol/value pair, and return the raw bind's result. Binding precedes the mirror;
the returned value is not substituted for the value written to that mirror.

This continues the primitive boundary established by 81AZ. Namespace factory v6
adds `publish` at offset 136 (144 bytes, sixteen required operations). There is no
additional service state. Id supplies a restricted mirror primitive alongside
its existing Symbol interner and transient RuntimeState context. Those primitives
retain Id's heap/vtable representation, weak-key policy, mutation accounting and
bootstrap behavior when internal dictionaries have not yet been installed.
All operations are synchronous: no sends, collection, reentry or retained borrows.

Runtime copies the selected publish/bind functions and storage pointer before
calling out. No RuntimeState or Environment reference spans an interner or mirror
callback. The actual production export path and bootstrap publication use this
operation. Id has no normal Rust dependency on the Namespace implementation.

Preserve existing C-name checks before runtime admission, owner/observation
checks, empty/non-UTF8 names, embedded-NUL rejection, Symbol compatibility limits,
ordered bindings, aliases, null values, weak mirrors, roots and identity. Neither
canonical export nor its primitives consult mutable public intern/import slots.
No new allocation safepoint, input validation or language behavior is introduced.
IdApi v9, native initializer transport and image encoding are unchanged. Removing
the remaining public import/export fields and migrating their live consumers is
still a separate boundary; do not introduce an aggregate compatibility table.

Before committing, check an independent native callback-order oracle including
distinct input/bind-result identities, failure without mutation, selected bind
and publish operations, moved/independent Runtimes and the actual weak mirror.
Compare separately compiled v5 baseline and v6 clients through their loaded
floors. Check required-operation admission, exact diagnostic precedence and GC
callback behavior. Verify dynamic startup, a source-free compiled caller, saved
native Code image and baked executable in a fresh debug prefix. No release
validation.

## Validation

- Workspace all-target check and debug installation passed. ABI, Namespace, Id
  and host library tests passed: 354 total, 17 intentionally ignored. Strict
  ABI/Namespace Clippy and syntax checks of 52 current native C fixtures passed.
- The independent C callback oracle checks separate storage/floor contexts,
  exact intern/bind/mirror order, borrowed bytes/extent, null input and output
  values, a bind result distinct from its input, rejection without subsequent
  callbacks or output mutation, and output publication after the mirror.
- The loaded-floor oracle passes 2,048 ordered-binding cases and 4,096 independent
  storage cases. Wrapped factory operations prove production/bootstrap use the
  selected publish and bind. A bind wrapper returning false instead of its input
  confirms the raw store and weak mirror still hold the original value, including
  null. Canonical publication ignores a replaced public intern callback.
- Separately compiled clients from `fa5088eb` and the current tree produce exact
  matching oracle transcripts through the actual v5 and v6 floors. The shared
  selected-bind/weak-mirror probe also passes against the original v5 source and
  libraries. No current descriptor is cast onto the historical factory.
- All six old/new boundary outcomes match exactly: null-name, owner-thread,
  null-name-before-owner, storage-observation, null-name-before-observation and
  allowed publication during GC root callbacks. The durable Namespace test gate
  checks these diagnostics and callback behavior.
- Runtime tests cover embedded-NUL rejection without Symbol, binding or mutation
  epoch changes; independent moved Runtimes; null rebinding; exact weak-mirror
  content and mutation counts; and the pre-dictionary no-op mirror. Existing
  precise-GC checks retain exported values and names and reclaim replaced values.
- All sixteen factory operations are mandatory; malformed descriptors and
  retargeting reject. Eighteen actual host admission cases pass, including the
  old provider name and missing/malformed prerequisite services.
- Fresh debug prefix:
  `/var/folders/f3/n4drgs5d7f3f6zxxxd2snx6m0000gn/T/birnam-81bc-prefix.f_v1h4xp`.
  All 1,801 source hashes remained unchanged through installation. Subsequent
  changes are this validation record and the README references only.
  The installed native oracle passes, and all 145 standard-library assertions pass.
- Dynamic startup, source-free compiled caller, saved v8 native Code image and
  baked executable pass `scripts/namespace-image-smoke.sh`. The native probe
  confirms that publish's code belongs to the loaded Namespace library; aliases,
  nil/null distinctions, identity, roots, reload and Globals separation remain.
- All 71 old/new library export inventories differ only in the two declared
  factory/dependency v5-to-v6 symbols. Both affected manifests, declared install
  names and system-only linkage pass. Bare Id's inventory is checked separately
  from the installed floor's existing module wrapper. Id has no normal Cargo
  dependency on Namespace. No release validation was run.

Receipts are `.cargo-target/81bc-{check,workspace-check,unit,clippy,namespace,
baseline,host-validation,install-driver,installed-native,inventory,image,stdlib}.log`
and `81bc-source-hashes.json`. Public import/export capability migration remains
the next Namespace boundary; IdApi is still v9 (32 fields, 256 bytes).
