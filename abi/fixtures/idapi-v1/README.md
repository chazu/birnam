# Frozen IdApi v1 compatibility fixtures

These arm64 macOS artifacts were frozen before the Rust floor implementation.
They are compatibility evidence, never bootstrap inputs:

- `idapi-v1-client.arm64-macos` dynamically loads exactly one selected floor,
  calls `id_init` once, and exercises early/late table fields, zero-argument
  buffers, callbacks, layouts, vtable reflection, and scalar values;
- `idapi-v1-module.arm64-macos.dylib` proves opaque `module_init` state threading
  through the selected floor's own `dlopen`/`dlsym` entries;
- `pre-rewrite.image` is archived v4 evidence containing canonical exported
  identities, leaf and layout-bearing values, compiled method code, a
  private-vtable receiver, and a block global. Current runtimes deliberately do
  not admit v4 images; its checksum preserves provenance rather than a boot
  compatibility promise.

The native ABI fixture gate verifies every checksum, runs the committed native
client and companion module against the current Rust floor, then separately
asserts that mapped and rebuilt admission of `pre-rewrite.image` both reject. It
never rebuilds any fixture. The ordinary build, install, runtime, and stage1
graph do not depend on this directory; temporarily moving the directory away
must leave those paths green.

`build-fixtures.sh <output-directory>` is a manual reproduction aid. Replacing a
committed binary or image remains an explicit provenance event: inspect the
diff, explain why old evidence is no longer authoritative, update `SHA256SUMS`,
and review the change separately. Do not run the build script from ordinary
tests and do not silently refresh artifacts after an ABI change.
