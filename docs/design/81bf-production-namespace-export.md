# 81BF — Namespace owns native export and its caller handoff

> **Status:** Implemented after 81BE (`9712a828`). Debug build, native caller
> migration, independent baseline comparisons and fresh installed startup,
> source-free compiled caller, saved-image and baked-executable checks pass.

The next operation is `IdApi.exportObj`. Namespace already owns the intern,
raw bind and weak-dictionary mirror sequence through 81BC's `publish` operation.
Id still owns the public C-name wrapper in `public_api::api_export_object`.
Move that entry into a Namespace-owned service and replace its aggregate-table
field with an explicit caller capability, following 81BE's lifetime discipline.
Keep basic Symbol interning and floor admission in Id.

## Observed boundary

At `9712a828`, `api_export_object` rejects null names, scans through the first
NUL, then obtains the process-published Runtime and invokes its byte-slice
`export`. Runtime performs owner/observation admission, copies the selected
publication operation/storage/bind, and calls Namespace without holding a
RuntimeState or Environment reference. Namespace interns, binds, mirrors the
original Symbol and input value, and returns the selected bind result. A null
input or null successful result remains valid. Globals is a separate registry.

Retain the internal byte-slice Runtime operation for Id's own bootstrap and
internal callers. It is not a replacement public compatibility table or
forwarding native export entry. The public C-name path must actually leave Id.

An initial Rust text inventory found 97 `exportObj` occurrences across 25
crates, including declarations, table construction and tests. This is a search
inventory, not a count of production calls. The raw locations are retained in
`.cargo-target/81bf-export-callers.json`. Each actual call and saved function
pointer needs an incoming/saved API and callback-timing classification before
its migration.

## Caller selection audit

- Integer storage snapshots `proto`, `alloc`, `exportObj`, layout and Object
  behavior together at the start of `Instance::populate`. Both later exports
  use that saved export function, even after prototype creation, owner-protocol
  callbacks or the first export. Snapshot the new export function/context at
  that same early point. Reading the live capability separately for those two
  exports would change behavior.
- Float, Word and String publish through their saved API, reading the export
  slot separately after owner-protocol callbacks and for each publication.
  Those reads must remain live at their existing positions; an Integer-style
  early snapshot would change their behavior.
- Collection coordinates Range, Vector and Map publication at separate
  construction positions. Their publication stages take the supplied API;
  transport the matching incoming export capability rather than substituting
  the module's saved method-execution API.
- ForeignAddress checks the first publication's result before publishing its
  prototype. Preserve the short circuit, including null successful bind results,
  and retain the incoming/saved selection of each actual caller.
- Prelude's initial System and Block publication, native initialization reentry,
  and later saved execution must remain distinct. Direct Native Loader must
  forward the saved execution capability through the initializer ABI without
  adding dependency queries or attachment.

The remaining callers were reviewed at their actual API selection points:

| Callers | Export selection |
| --- | --- |
| Codegen, Lang, Tcc, Artifact | Capture with first saved Runtime API; initialization publications use that saved selection too. |
| Image, Store, PEG, Error, Isolate, StringBuilder | Incoming initialization API, including publications after Runtime creation. |
| Alien | Incoming registration/publication; method-side ForeignAddress allocation saves exports with the earlier API OnceLock. |
| Relational | Incoming registration helpers, including final db publication. Method execution does not export. |
| Portable Code | Capture with its first installed Runtime API; byte-slice export uses that saved capability. |
| Host Project bridge | Loaded stack's matching exports, retained alongside the selected floor. |
| ForeignAddress | Tcc/Alien construction uses incoming exports; Codegen/Tcc/Alien method allocation uses matching saved exports. Preserve short circuit and per-publication selection. |

No actual export operand evaluates a callback between the old callee-slot read
and its invocation. The explicit helper therefore retains each original read
position; Integer's tuple snapshot is kept separately. Native numeric and warm
Block declarations never call export on their admitted paths, so remain v4.

## Implemented extraction

The mutable 32-byte `NamespaceExportV1` descriptor contains version, extent,
opaque context and `export(context, const char *name, Oop value) -> Oop`.
Namespace owns its canonical service allocation. An immutable copied floor
descriptor supplies admission, restricted interning and weak-dictionary mirror
callbacks. Creation/destruction must preserve independent services and remain
independent of mutations to the public function/context fields.

`exportObj` is removed from IdApi v11: 30 fields, 240 bytes. Native
initialization now takes five arguments. Namespace factory v8 is 176 bytes
with twenty operations; its added export creation/destruction entries are at
160/168. Float/Word prepare v4 append exports after imports; String prepare v2
receives `(api, exports, allocation)`. Range/Vector/Map v4 change only the
publication-stage signature to `(api, exports)`. Native Loader installation v4
adds the saved execution exports after imports. Other installers remain v3.
The independent v11 manifest fingerprint is
`1224e6a2fb37bf2273fcbf47650072c2b66b49d9072ca620dcf72490e49d6e0e`.

No aggregate facade, API-address registry, process-global replacement selection,
or production slot loan is permitted. A module saves exports at the original
execution-API selection point. Registration/adoption follows the incoming API
where the original code did so. Snapshot versus live operation selection follows
each original read, including the Integer exception above. Preserve root
registration, partial identities, allocation and callback order, fresh dispatch,
native code lifetime, reentry and reload behavior.

## Completion evidence required

Use completed 81BE and its frozen debug prefix as the independent implementation
baseline. Compile historical C fixtures from that commit with its v10 header;
current fixtures must use the new independent header and explicit capabilities.
Add adversarial cases that replace export during construction and between
publications, including Integer's early snapshot and the other families' live
reads. Preserve exact transcripts, return values and short circuits.

Check null-name/owner/observation diagnostic precedence, empty/non-UTF8/first-NUL
names, missing-name interning, aliases and null/nil bindings, weak mirrors,
callback GC, moved independent runtime state, capability copying and destruction.
Verify ABI/factory admission, actual library exports and all affected native
initializer/preparation callers, not just the service in isolation.

Before committing this extraction, build a fresh external debug prefix from
frozen sources and verify dynamic startup through siphon, source-free compiled
callers, saved native Code images and baked executables. Keep the 81BE worker
deadline and broader all-features limits visible; do not alter production
semantics to make a validation command pass. No release validation is required.


## Validation evidence

- Default debug workspace build and all-targets check pass.
- ABI/Namespace/Id library tests: 280 passed, 16 ignored.
- Host library tests: 86 passed, 1 ignored. Affected Id/host/Codegen
  all-targets/all-features checks pass.
- All 56 current C units compile under strict warnings; the two storage probes
  also compile. Historical v10 source is independently archived from `9712a828`.
- Sixteen native consumer traces and twenty collection traces match the frozen
  81BE prefix, including registration reentry, live replacement, adoption,
  callback GC, copied methods and reload. Native Loader additionally checks
  that fifth-argument export handoff retains Prelude's original descriptor.
- Eight constructor-selection cases match independently compiled v10/v11
  clients: replace during construction or between publications in Integer,
  Float, Word and String, with callback GC and retained identities. Integer
  uses its first function/context twice; the other families use live selections.
- The Namespace native suite passes ordered bindings, independent states,
  factory admission, null/owner/observation diagnostics and root callbacks.
  The final tracked runner also passes all eight export-selection cases.
- A fresh external debug prefix installs 43 native libraries and 28 stage-1
  modules from 1,863 unchanged source hashes. After installation, only a stale
  test-fixture factory assertion and documentation were corrected; runtime and
  build sources remained unchanged.
- All 71 actual installed export inventories match 81BE plus the intended
  version changes and export provider. The ten existing affected/dependent
  manifests match symbols, install names and system-only dependencies.
- The independent IdApi v11 C client/module passes all 30 fields and the
  five-argument initializer handoff. All 28 host factory/capability admission
  cases pass, as do installed closure fallback and warm Block admission probes.
- Installed stdlib: 145 passed, 0 failed.
- All eight installed image suites pass dynamic startup through siphon,
  source-free compiled callers, v8 saved native Code images and baked
  executables: Namespace, Error import, Error presence, Namespace enumeration,
  Word Error Value, Native Loader, Source Loader and Block Loop.
- Baseline/current image-backed workers produce byte-identical transcripts for
  twelve wire round trips and orderly Stop/Stopped. Debug ready times were
  24.96 and 24.50 seconds respectively.

Evidence is under `.cargo-target/81bf-*`. The broader workspace all-features codec
configuration limit and debug worker-service's production 10-second ready
deadline remain the documented 81BE limits. This extraction does not claim that
the broad all-features check or full worker-service suite passes. No production
workaround or release validation is introduced.
