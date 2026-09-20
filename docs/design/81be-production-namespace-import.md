# 81BE — Namespace owns import and its native handoff

> **Status:** Complete following 81BD. Rust and current native C consumers use
> explicit matching import capabilities. Debug dynamic startup, source-free
> compiled callers, saved native Code images and baked executables pass.

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

## Implementation and validation

Namespace now implements the owned import service and exposes factory v7. Id
owns its live and canonical import descriptors, publishes the explicit provider,
and removes the import field from the Rust IdApi v10 mirror and new manifest.
Native initializer declarations now accept the fourth import pointer. Numeric
and warm-Block admission declarations move to v4 with explicit import context.

The first consumer group threads the capability through Integer, Float, Word,
Prelude, the six System/Block installers, and direct Native Loader initialization.
Error Value wrappers and ForeignAddress helpers now require explicit imports;
the temporary production import-slot loan is removed. Collection, StringBuilder,
Error, Lang, Store, PEG, and Artifact transport explicit imports. Range,
Vector, and Map preparation moves to v3 to receive the incoming capability.

The remaining Rust consumers now carry matching incoming or saved imports:
Store, Tcc, Alien, Codegen, Image, Isolate, Relational, and host startup/project/
interaction helpers. Isolate's worker driver and OOP codec native bridges move
to v2 and receive the caller's import capability explicitly. Rust initializer
calls, test constructors, and the numeric semantics declaration fixture have
been migrated. IdApi v10's manifest indices and fingerprint now agree with the
independent ABI unit test.

Current independent C headers now describe IdApi v10, Namespace v7, and the v3
installer/collection preparation APIs. Shared fixture support transports an
explicit import capability. All 55 current C fixture translation units pass the
strict syntax check.
Instrumented callers use independent explicit descriptors, including copied
API registration, reentry and live callback replacement. Current export
manifests and ABI documents describe the new signatures and versions.

Validation against completed 81BD commit `22de2f58`:

- Debug `cargo check --workspace --all-targets --offline --locked` passed.
- ABI, Namespace, and Id library tests passed: 284 passed, 16 ignored. Coverage
  includes import ordering, mutable descriptors, callback reentry, copied
  capabilities, independent moved runtimes, and service destruction ownership.
- Loaded Namespace output matched completed 81BD commit `22de2f58` for the
  ordered binding and independent-state oracles. All six import-boundary cases
  matched exit status and exact output, including diagnostics and GC callbacks.
- The complete Namespace script passes storage/provider, presence, publication
  and import boundaries, including exact diagnostics and GC callback behavior.
- Sixteen Error Value/numeric/System/Block consumer traces and twenty collection
  traces match 81BD exactly. Numeric declaration tests verify import function
  and context mismatches decline without invoking them, then restore admission.
- Host library tests pass: 86 passed, one ignored. The default debug workspace
  build and affected Id/host/Codegen all-targets/all-features checks pass. The
  feature-only closure timing fixture now selects its Runtime's String cleanup
  instead of referring to the removed production-floor cleanup shim.
- A broader workspace all-features invocation fails when it enables the
  provider-free artifact codec's `module` feature without an ABI dependency.
  No change to that feature configuration is included in this extraction.
- All 23 host Namespace factory/names/presence/import admission cases pass,
  including rejection before floor initialization and malformed/missing imports.
- A fresh external debug prefix built 43 native libraries and 28 compiled stage-1
  modules from 1,841 unchanged source hashes. All 71 old/new export inventories
  match the intended renames plus Id's import provider; fifteen affected native
  manifests match actual exports. Integer's historical absolute install name
  remains unchanged.
- The loaded independent IdApi v10 C client/module, sixteen installed consumer
  trace comparisons, and closure fallback callback oracle pass. Warm Block and
  numeric v4 admission reject changed import functions and contexts without
  invoking them; restoration admits again. Cold/absent Block refusal also passes.
- Eight scripts pass startup, source-free compiled caller, v8 native Code image
  and baked execution: `namespace`, `error-import`, `error-presence`,
  `namespace-enumeration`, `word-error-value`, `native-loader`, `source-loader`,
  and `block-loop` (`scripts/NAME-image-smoke.sh PREFIX`). They check the loaded
  implementation owner as applicable, aliases, roots/GC, callback snapshots,
  live method/factory replacement and native reload.
- Installed standard library tests pass: 145 passed, zero failed. The closure
  elision/observation smoke passes ordinary, enabled and forced recovery paths.
- `scripts/isolate-image-wire-test.py PREFIX --baseline OLD_PREFIX` uses an
  independent wire reader and the actual image-backed host worker. It deletes
  source before worker boot and compares twelve scalar/nested Vector/Map replies
  plus orderly stop byte for byte. Both prefixes pass. Observed debug ready
  times were roughly 28-33 seconds, beyond the unchanged production parent's
  ten-second deadline. The ordinary worker-service smoke fails with the same
  `handshakeTimeout` on both prefixes; Cell mapped/rebuilt recovery passes on
  both. The external wire test does not change that deadline or claim the full
  nineteen-case worker-service suite passed.

Reproduction uses a fresh `scripts/rust-install.sh install --profile debug`
prefix, the focused native scripts, the eight image scripts above, and the
independent worker wire test. Historical native traces are compiled from 81BD's
own C headers and fixtures against its retained debug prefix, independently of
the current C sources. Local command outputs are retained under
`.cargo-target/81be-*.log`; the source freeze is
`.cargo-target/81be-source-hashes.json` and the prefix is recorded in
`.cargo-target/81be-prefix-path`. The final documentation and reusable wire test
were added after the install freeze; runtime/build sources were not changed.

The broader all-features configuration failure and worker-service deadline
failure above are explicit validation limits. No release validation was run.
Language behavior, public selectors, object identities, representation and
image encoding remain unchanged; native ABI consumers must rebuild.
