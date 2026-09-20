# 81BF — Namespace owns native export and its caller handoff

> **Status:** Boundary and caller audit started after completed 81BE
> (`9712a828`). Implementation and validation remain open. No completed
> extraction or additional production ABI change is claimed here.

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

## Selection rules found so far

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

The remaining Codegen, Image, Alien, Isolate, Store, Relational, Tcc, Lang, PEG,
Artifact, StringBuilder, host and portable-code uses still require individual
review. Do not infer one universal snapshot rule from the common field name.

## Intended extraction

Use a mutable 32-byte `NamespaceExportV1` descriptor containing version, extent,
opaque context and `export(context, const char *name, Oop value) -> Oop`.
Namespace owns its canonical service allocation. An immutable copied floor
descriptor supplies admission, restricted interning and weak-dictionary mirror
callbacks. Creation/destruction must preserve independent services and remain
independent of mutations to the public function/context fields.

Remove `exportObj` from IdApi rather than lending its old slot. IdApi would
become v11, with 30 fields and 240 bytes. Native initialization must transport
exports explicitly with the existing presence/import pointers. Version the
preparation and construction/installer contracts whose signatures actually
change, including early String/numeric preparation and direct Native Loader.
Keep unaffected operation descriptors and optional native declarations at their
existing versions unless an actual signature or contract change requires it.
Confirm the exact factory/preparation versions during caller review.

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
