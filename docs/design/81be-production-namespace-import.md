# 81BE — Namespace owns import and its native handoff

> **Status:** Partial implementation checkpoint, following Error Value handoff in
> 81BD. Saved and pushed at the user's request; the full workspace does not yet
> compile. This is not a completed extraction.

Move canonical C-name import into Namespace and remove IdApi.import. Namespace
creates one mutable import descriptor per Runtime, using selected lookup/find,
raw storage and copied restricted floor admission/interner callbacks. Preserve
C-string scanning and null-name diagnostics before owner/observation admission,
missing-name interning, empty/non-UTF8 names, raw-null results and independent
moved runtimes. Destruction uses the owned service address, not mutable context.

IdApi becomes v10 (31 fields, 248 bytes). Native module_init gains a fourth explicit
import pointer after presence. Update all current callers, compiler/native entry
bridges and independent current C fixtures without old-signature casts. Keep the
incoming capability alongside incoming API registration and capture import at the
original first-saved execution API selection, including early initialization and
reentry. Float/Word preparation and the six System/Block installers transport the
same selection explicitly. Direct Native Loader forwards it without additional
symbol queries or dependency mounting.

Migrate actual direct import consumers, Error Value's wrappers and ForeignAddress.
No aggregate compatibility table, API-address registry, process-global replacement
selection or production slot loan may remain. Per-call capability operations
snapshot live function/context at each original call; imported objects retain
snapshot behavior through later callbacks. Preserve fresh dispatch, alias/identity,
GC/native lifetime, roots, adoption/reload, errors and image encoding.

Numeric and warm-Block admission must compare canonical import function/context
against saved live capabilities without invoking them. Capture original canonical
operands independently of the mutable descriptor and update versioned declaration
signatures. Other Id primitives and Id's internal byte-slice import used by its
own runtime remain unchanged; they are not public forwarding facades.

Audit every caller's actual API selection, compare independent 81BD/current native
transcripts and exercise explicit capability changes during registration/execution
and reentry. Check factory/service admission, exact boundary diagnostic precedence,
missing-name interning, null/nil bindings, GC callbacks, copied capabilities and
pure declaration admission/restoration. Verify dynamic startup, source-free
compiled callers, saved native Code images and baked executables in a fresh debug
prefix before marking the extraction complete. No release validation.

## Checkpoint progress

Namespace now implements the owned import service and exposes factory v7. Id
owns its live and canonical import descriptors, publishes the explicit provider,
and removes the import field from the Rust IdApi v10 mirror and new manifest.
Native initializer declarations now accept the fourth import pointer. Numeric
and warm-Block admission declarations move to v4 with explicit import context.

The first consumer group threads the capability through Integer, Float, Word,
Prelude, the six System/Block installers, and direct Native Loader initialization.
Error Value wrappers and ForeignAddress helpers now require explicit imports;
the temporary production import-slot loan is removed. Collection, StringBuilder,
Error, Lang, Store, PEG, and Artifact have partial caller migration. Range,
Vector, and Map preparation moves to v3 to receive the incoming capability.

Debug `cargo check` passed for ABI/Namespace and the first consumer group.
The workspace check failed on remaining Store and Tcc ForeignAddress callers
and Tcc's removed import-field access. These are unfinished migration work,
not baseline failures. No runtime or image validation has been performed for
this checkpoint.

Remaining work before completing 81BE:

- Finish Store's ForeignAddress call and migrate Tcc, Alien, Codegen, Image,
  Isolate, Relational, and host startup/interaction helpers. Preserve each
  caller's incoming or saved API/capability choice, including initialization
  reentry and adapters that carry their own API.
- Migrate direct Rust initializer calls, test constructors, native C fixtures,
  current C headers, generated bridges, provider/dependency manifests, and
  numeric/Block declaration fixtures to the new signatures and versions.
- Add independent import-service and handoff coverage, compare native traces
  against completed 81BD commit `22de2f58`, and finish ABI/factory admission
  and diagnostic checks.
- Run focused debug checks and fresh-prefix dynamic startup, source-free,
  saved-image, and baked-image validation; then update current ABI and user
  documentation with the verified result.
