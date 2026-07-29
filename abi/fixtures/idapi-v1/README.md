# Frozen IdApi v1 compatibility fixtures

These arm64 macOS artifacts were frozen before the Rust floor implementation.
They are compatibility evidence, never bootstrap inputs:

- `idapi-v1-client.arm64-macos` dynamically loads exactly one selected floor,
  calls `id_init` once, and exercises early/late table fields, zero-argument
  buffers, callbacks, layouts, vtable reflection, and scalar values;
- `idapi-v1-module.arm64-macos.dylib` proves opaque `module_init` state threading
  through the selected floor's own `dlopen`/`dlsym` entries;
- `pre-rewrite.image` contains canonical exported identities, leaf and
  layout-bearing values, compiled method code, a private-vtable receiver, and a
  block global whose value is deferred and re-derived from recorded source.

The Rust workspace's ABI fixture test verifies checksums and runs the committed
artifacts against the current Rust floor. It never rebuilds them. The ordinary
build, install, runtime, and stage1 graph do not depend on this directory;
temporarily moving the directory away must leave those paths green.

`build-fixtures.sh <output-directory>` is a manual reproduction aid. Replacing a
committed binary or image is an explicit ABI/image compatibility event: inspect
the diff, explain why old evidence is no longer authoritative, update
`SHA256SUMS`, and review the change separately. Do not run the build script from
ordinary tests and do not silently refresh artifacts after an ABI change.
