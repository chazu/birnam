# Optional provider loading and production cutover

2026-10-20. Next implementation contract; the following packaging and production
changes are not yet delivered. Image and documentation boundaries are already
qualified in the Store-independent core completion plan.

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
A user image whose provider is unavailable can reject, as the current image
contract allows; automatic provider acquisition is not required by this cutover.

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
