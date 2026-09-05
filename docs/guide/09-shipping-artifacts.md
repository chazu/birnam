# Shipping compiled artifacts

Birnam keeps source authoring, local publication, and installed execution
as three separate boundaries:

| Boundary | Artifact | Purpose |
| --- | --- | --- |
| Source | `birnam.toml` and `.birnam` files | Declare project roots, origins, source composition, wrapped builds, and developer run policy. |
| Local build | module, configuration, application, and native artifacts | Atomically publish a self-describing executable closure. |
| Installed runtime | module dylibs, native siblings, or images | Describe and execute their own runtime closure without project/build TOML. |

There is no generated project-build receipt. `birnam build` inspects previous
artifact descriptors to make incremental reuse decisions, then atomically
replaces `.birnam/build/<target>/<profile>/`. `birnam install` copies the
descriptor-discovered closure, not the project manifest or source files.

## What a module artifact describes

Every production module dylib embeds a pointer-free descriptor containing:

- its artifact kind and canonical module identity;
- the compiled-artifact ABI and architecture/operating-system target;
- selectable root identities;
- canonical Birnam module dependencies;
- adjacent native-library requirements;
- capabilities available after activation; and
- source and executable-revision digests.

The descriptor is ordinary ID surface. Scalar fields use messages such as
`kind`, `identity`, `artifactAbi`, and `target`. The minimal list interface is:

```text
rootsCount / rootAt:
requiresCount / requireAt:
nativeRequiresCount / nativeRequireAt:
capabilitiesCount / capabilityAt:
```

Those messages need only `libid` and `libbirnam_id_artifact`. After Collection
is active, `roots`, `requires`, `nativeRequires`, and `capabilities` return
Vectors over the same data.

In a running Birnam environment, the descriptor can be inspected without
activating its artifact:

```birnam
let description := [Artifact inspect: "/path/to/libbirnam_app.dylib"];
[description identity];
[description requires];
[description nativeRequires]
```

If inspection answers nil, `[Artifact lastFailure]` identifies the failure.

## Selecting a compiled root

Pass a dylib directly to the siphon:

```sh
birnam /path/to/my-root.dylib
```

Only the explicitly selected root may have an arbitrary filename. Its module
dependencies resolve beside it as `libbirnam_<identity>.dylib`; native
requirements resolve as their exact declared basenames in the same directory.

Before activation, the siphon:

1. loads `libid` and the artifact inspector;
2. reads the selected root without `dlopen`;
3. recursively inspects its sibling dependency closure;
4. rejects a wrong kind, identity, ABI, target, dependency cycle, or unavailable
   native requirement; and
5. only then loads the language activation stack and installs the complete
   dependency-first plan.

Inspection does not run constructors, `__birnam_unit_init`, `__ev`, module
initializers, or entry behavior. Planning the complete graph first prevents an
invalid later sibling from leaving an earlier dependency partially installed.

Direct dylib selection activates the root module's own top-level program. A
project build captures its resolved module programs and `main` in an explicit
application root, so ordinary `birnam run` can execute the last successful
publication without consulting project source. Use `birnam run --build` when
the publication should first be refreshed from current source.

## Project build and installation

The source workflow remains:

```sh
birnam check /path/to/project
birnam build /path/to/project
birnam run /path/to/project
birnam test /path/to/project
birnam install /path/to/project
```

`check` and `build` use `birnam.toml`. Check compile-validates every module in a
private temporary directory and offers stable `--format=json` output. `build`
publishes a complete local directory atomically. Ordinary `run` selects and
preflights the compiled application root, reads run policy from its compiled
configuration root, and activates the application exactly once without source
observation.

Build, run, test, and install select independent `debug` or `release`
publications with `--profile`. `test` discovers or reads `[test].sources` and
runs each test source in a fresh process against the validated compiled closure.

`install` validates the same publication and copies the union of its module,
application, configuration, and native closures into the selected runtime
library directory. The installed result contains no project manifest or source
inventory. Installed module identity and dependency planning come from the
descriptors.

The standard Birnam CLI, REPL, LSP, documentation, and library modules follow
the same rule: their authored files live in the repository, while install and
release bundles contain compiled units with embedded source/reflection data.

## Bundle layout and relocation

The supported runtime layout is fixed relative to the executable:

```text
prefix/
  bin/birnam
  lib/libbirnam_id.dylib
  lib/libbirnam_id_artifact.dylib
  lib/libbirnam_*.dylib
  share/birnam/src/Cargo.{toml,lock}
  share/birnam/src/rust/birnam-abi/Cargo.toml
  share/birnam/src/rust/birnam-abi/src/*.rs
  share/birnam/src/rust/birnam-id/{Cargo.toml,build.rs}
  share/birnam/src/rust/birnam-id/src/*.rs
  share/birnam/docs/guide/*.md
```

Moving `prefix/` as a unit preserves lookup. `BIRNAM_LIB_DIR`,
`BIRNAM_ID_DYLIB`, and `BIRNAM_RUST_WORKSPACE_DIR` are narrow test/embedding
overrides, not required runtime manifests. The last names the workspace root
containing `Cargo.toml`, not an individual crate's `src` directory. Production
lookup does not fall through to the current directory or a source checkout.

## Native wrappers

A project authors a `[[wrapped]]` build recipe in `birnam.toml`. The generated
Birnam wrapper artifact declares the native dylib it opens, and the native dylib
is published beside that wrapper. Local project dependencies propagate these
recipes transitively in dependency-first order; each recipe runs from its own
project root and all outputs share the root build's `lib/` namespace. Duplicate
wrapped names are rejected. Inspection checks presence before any wrapper
initializer runs. Release staging copies optional wrappers only as deliberate,
complete, signed closures; it never discovers them from developer build dirt.

## Integrity and failures

The embedded descriptor is routing and compatibility metadata, not an external
checksum manifest. Explicit build compares current source metadata with the
previous publication's descriptors for incremental reuse. Installed/release
integrity belongs to the platform artifact and signing boundary; release staging
signs and verifies each dylib and the siphon executable.

Common pre-activation failures include:

```text
artifactUnavailable
artifactDescriptionMissing
artifactKindMismatch
artifactIdentityMismatch
artifactAbiMismatch
artifactTargetMismatch
nativeRequirementUnavailable
circularDependency
```

Images use their checked image header and embedded code records rather than a
sidecar descriptor file. Compatible boot maps code; `--rebuild` is the explicit
source-recovery path.

For the normative architecture, see [Compiled Units Are
Primary](../design/45-compiled-units.md) and [Relocatable Bundles and Runtime
Distribution](../design/54-relocatable-bundles.md).
