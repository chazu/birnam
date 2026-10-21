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

Bootstrap constructs Artifact after the value, collection and Execution owners.
Native embedders pass the completed Namespace to its initializer; no dependency
table is attached. `roots`, `requires`, `nativeRequires`, and `capabilities`
return Vectors over the same data. See the
[Artifact message contract](../../rust/birnam-artifact/README.md).

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

A project authors a `[[wrapped]]` build recipe in `birnam.toml`:

```toml
[[wrapped]]
name = "foo"
packaging = "embedded" # or "adjacent" (the default)
build = [["cc", "-dynamiclib", "native/foo.c", "-o", "{output}"]]
```

The binding module declares its native requirement and prepared bindings:

```birnam
[Module named: #foo requires: #[]];
[Alien dlopen: "libfoo.dylib"];
let Foo := [Object clone];
[Alien bind: "foo_version" signature: "i" as: #version on: Foo]
```

`bind:signature:as:on:` is an explicit artifact-compilation declaration when
`Alien` is unshadowed and function/signature are literal Strings. Codegen asks
the current AlienCompiler to prepare object code while compiling the module.
Loading resolves the current native address and installs/records an ordinary
Method, with no signature generation or C compilation. Source evaluation can
still execute the declaration interactively. Use `Alien wrap:` for dynamically
computed runtime bindings; its normal dispatch and compilation behavior remain.
An invalid prepared signature fails the build without activating the module or
running native constructors. New `birnam wrap` scaffolds use `bind:`.

| Packaging | Published component | Native lookup at activation |
| --- | --- | --- |
| `adjacent` | Birnam artifact plus native dylib sibling | Its declared sibling, independent of caller cwd/library directory |
| `embedded` | Birnam artifact contains the compiled bindings and declared native library bytes | Alien materializes private native siblings and the OS opens them |

An embedded wrapper with no other Birnam module dependencies can be copied as
one file. It still uses the installed Birnam runtime. Other Birnam dependencies
remain ordinary artifact dependencies. `nativeRequires` lists only external
native siblings; embedded payloads are carried in the compiled initializer.
Payloads use a binary-safe hex carrier so native contents cannot impersonate an
artifact descriptor. Embedded artifacts are identified by
`embedded-native-hex-v1`; the current compiler marks its preparation capability
with `prepared-alien-v1`. Older runtimes reject the extended initializer record.

Every native library to be carried must be declared in `[[wrapped]]` and in the
module's literal native requirements. OS-level dependencies must remain
relocatable; package `@loader_path` native siblings together. This does not
implicitly bundle system libraries, external resources, or arbitrary dependencies
discovered by running constructors. Embedded inputs are limited to 256 MiB each.
Conflicting content under an already-provided logical native name is rejected.
Private materialization is owned by Alien and removed on normal runtime/DSO exit;
adjacent files remain caller-owned. Module packaging does not change Image's
separate native-library recovery contract.

Local project dependencies propagate wrapped
recipes transitively in dependency-first order; each recipe runs from its own
project root and all outputs share the root build's `lib/` namespace. Duplicate
wrapped names are rejected. Inspection checks presence before any wrapper
initializer runs. Release staging copies optional wrappers only as deliberate,
complete, signed closures; it never discovers them from developer build dirt.

Explicit builds run the native recipes before compiling embedded consumers.
Embedded publications conservatively recompile to capture native-only edits;
switching packaging also invalidates affected reuse. Failed native builds preserve
the previous publication. `check` validates declarations without running native
build commands. `run` and `install` consume the published closure without source
or compiler work. Inspecting either mode remains inert.

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
