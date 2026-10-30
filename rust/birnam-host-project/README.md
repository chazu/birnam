# Read-only project observation over Id

`HostProject` is a separately loaded native owner implemented in Rust. It owns
project discovery, TOML/manifest observations, source composition/indexing,
read-only dependency/cache inspection and ProjectPolicy resolution. Queries do
not fetch dependencies, run wrapped build commands, compile artifacts or publish
build outputs. Missing Git dependencies require an explicit ProjectBuild fetch.

Its constructor receives Namespace and the existing five resource paths through
the standard Host owner protocol. Current Id lookup resolves all dependencies;
its receiver and method contexts have process lifetime.

The ordinary query interface includes `root:`, `manifest:`, `snapshot:`,
`resolveSnapshot:`, `dependencyPaths:dependencies:`, `index:`,
`observe:root:composition:`, `observeBytes:path:root:`, `embeddedFiles:` and
`testSources:`. Snapshot, manifest, resolution, source and dependency observations
used by ProjectBuild pass the existing project-policy Maps and Vectors. Snapshot
identity is preserved through resolution; source bytes remain String values.
Only the executable's command-result adapters still use JSON. Filesystem paths
retain raw bytes where that was already the contract. `support`
loads the existing authored TOML and ProjectPolicy modules when necessary.

LSP calls `pathForUri:` and `snapshotForUri:` directly. These answer the existing
path String and resolution Map/error Map, respectively. HostInteraction owns
editor entry; no temporary Project global, stack-borrowed native callback,
revocation guard or project-to-interaction startup detour remains.

[ProjectBuild](../birnam-project-build/README.md) owns check/build/fetch/install and
published execution. Shared local [value helpers](../birnam-project-support/README.md)
contain no native owner state. See the
[historical extraction report](../../docs/archive/2026-10-runtime-refactors/native-module-decomposition-roadmap.md).
