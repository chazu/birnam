# Optional provider loading and production cutover

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-20. Implemented production contract. Core absence, late provider
activation, optional images/workers and cold archive checks pass. Host
edge-case and timing qualification also pass and are recorded in the
[completion plan](store-independent-core-completion.md). Delivered in
[v0.18.0](https://github.com/chazu/birnam/releases/tag/v0.18.0) after clean-candidate
and immutable-archive acceptance.

Use ordinary compiled Birnam modules as the public entries for `#store` and
`#relational`. Their initialization sends the existing `System loadModule:arguments:`
protocol to load the native provider with Namespace as its opaque argument. Store's
source initializer also installs its image and fact-reflection integration;
Relational declares Store as a dependency. Doc declares both data dependencies.
This keeps dependency planning in the existing Module/Artifact protocols, rather
than adding a native service registry or another dependency resolver.

The authored entry and native provider cannot both occupy `libbirnam_store.dylib`
in the same directory. Put the optional native providers under `lib/native/`,
retaining their native library basenames, and put the compiled authored entries
at the normal `lib/libbirnam_<module>.dylib` paths. Keep the existing native ABI,
exported owner names and Id-only interoperability. Native integration fixtures
must explicitly select the native provider path; they must not accidentally pass
a compiled Birnam entry to the native initializer protocol.

Build and prove the core first. Core native packages/bootstrap inputs exclude
Store and Relational; core authored activation excludes their integration and
Doc. Prepare and probe the core startup image before installing/activating the
optional providers. Build/package those separately, then prepare the explicit
catalog/tool composition. Shipping optional libraries is allowed; the core must
not load them merely because they are present. Record the actual closures and
update installer inventory, relocation, archive and release checks together.

Ordinary source, project, console, socket, worker and LSP paths use the core
composition. Documentation commands may select a prebuilt catalog composition,
but its required providers must be explicitly loaded before image application.
A core image must never trigger a Store probe or manufacture an empty database.
Image declarations activate their required local modules before heap restoration.
Unavailable providers reject; automatic package download is outside this cutover.

Late activation must preserve the live Globals facade, Cells, current methods,
existing aliases and source metadata. Ordinary edits before and after loading
Store must create no facts. Repeated Module loading must be a no-op and must not
repair user replacements or reselect an image owner after the user changes it.
Native Store/Relational self-description and explicit queries/history remain.
An explicitly repeated initializer is still an ordinary user operation, with
its existing partial-effect behavior; no new rollback or repair policy is added.

Required evidence: source and compiled core bootstrap with provider libraries
physically unavailable; a cold core build without their bootstrap inputs; actual
late `Module load:` of Store and Relational; preservation/replacement/GC and
self-description tests; core and optional mapped/rebuilt/resaved images; real
Host/CLI/LSP/socket/worker operation; installed relocation/archive validation.
Only after these gates pass is the full absence gate promoted to mandatory and
a new release qualified through the existing release workflow.

## Image closure refinement (2026-10-20)

The first cutover passes real core absence and late activation. It also exposes
an incomplete image contract: a Store owner reference alone does not restore
optional native behavior (notably Relational's methods) in a fresh worker. Manual
provider setup is insufficient for ordinary saved-image/worker execution.

Use the existing Module registry to describe successfully activated native
providers with an ordinary `imageProvider` field. Store and Relational set this
field in their authored initializers. Module supplies a dependency-ordered list;
there is no parallel native registry or ambient Store probe. Native embedding
callers may explicitly register their already-loaded provider in the same owner.

Add one optional v8 framing record, kind 12, before Sources/Behaviors/heap data.
Its payload is a u32 count followed by canonical Symbol fields naming providers,
with a limit of 4096 and no duplicate names. New runtime images always include it,
including an empty record for the core. The format reader may inspect older
framing fixtures, but live loading requires the declaration; older executable
images must be rebuilt. The provider declaration contributes to composition
identity. Update the independent reader and malformed-input oracle separately.

Before heap restoration, Image sends the declared list to current
`Module loadImageProviders:`. That early protocol validates each artifact through
Artifact, activates it through Codegen, and marks it loaded in the existing
registry; it does not require stdlib or run a second dependency planner. The
saved list is already dependency-ordered, and artifact admission verifies its
requirements. Missing providers reject before image application. Prior provider
activation effects are not rolled back if a later provider fails. This is local
activation of declared code, never package download or implicit discovery.

This also removes the temporary Host-specific catalog-provider loop: all image
entry paths, including workers, use the same declared closure. Core declarations
are empty and therefore cause no optional module activation. Fresh-process
optional images/worker programs must be tested, alongside provider absence,
activation failures, reentry/collection and the existing partial-load boundary.
