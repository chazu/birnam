# Project build and publication over Id

`ProjectBuild` is a separately loaded native owner implemented in Rust. It owns
compilation sequencing, compiler checks in disposable directories, reuse identity
validation, generated module/configuration/application construction, wrapped
commands, filesystem staging/publication, explicit dependency fetch, installation
and project test/run orchestration. Its constructor uses the existing Host owner
protocol and process-lifetime resource Context.

The public messages are `check:`, `fetch:`, `build:target:profile:version:`,
`locate:profile:`, `install:directory:`, `test:source:profile:` and `run:version:`.
The CLI's Rust value adapter calls this owner directly; HostProject has no aliases
for these operations.

Build requests query [HostProject](../birnam-host-project/README.md) through Id for
ordinary snapshot, manifest, resolution and source records. A snapshot stays an
Oop through policy resolution; native build code decodes the result once when
it needs to operate on files. These internal queries do not encode/decode JSON
or reconstruct the snapshot. No private payload crosses that interface. ProjectPolicy continues to decide providers, ordering and
compile-versus-reuse actions. Codegen produces artifact bytes; HostEntry loads and
executes published code; HostInteraction owns REPL/socket entry.

The previous algorithms and publication order are retained: validate complete
artifacts, write a staging tree, then rotate the prior publication aside and
rename the stage into place with restoration on failure. Build reuse compares
exact source, revision, provenance, tool/target/profile and artifact checksums.
Explicit fetch retains pinned clean-checkout validation and stage publication.

See the [historical extraction report](../../docs/archive/2026-10-runtime-refactors/native-module-decomposition-roadmap.md).
