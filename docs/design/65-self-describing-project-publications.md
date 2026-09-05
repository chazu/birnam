# 65 — Self-Describing Project Publications

> **Status:** Implemented 2026-09-03; source/publication acceptance completed
> 2026-09-05 (`birnam-mtd`). This is not a new release qualification.
>
> This design replaces the generated `artifacts.toml` build receipt from
> Designs 51, 52, 54, and 57. Compiled artifacts are the publication authority;
> source observation belongs to `check` and `build`, never ordinary `run`.

## 1. Problem

Birnam's compiled modules already embed inert descriptors containing their
identity, artifact ABI, target, source and revision digests, Birnam requirements,
native requirements, and capabilities. Installed execution correctly treats
those descriptors as authority.

The source-project workflow nevertheless writes `artifacts.toml`, reparses it
through the self-hosted TOML implementation, reconstructs the complete source
graph, hashes artifacts repeatedly, and then inspects their embedded descriptors
again. A warm `birnam run` of Digla takes roughly twelve seconds before its
application code starts. The generated receipt duplicates artifact facts and
creates a second authority contrary to the compiled-unit model.

## 2. Decision

There is no project build receipt. In particular, a successful build publishes
no `artifacts.toml` or equivalent sidecar inventory.

`birnam build` is the explicit transition from authored source intent to a
self-describing project publication. `birnam run`, `birnam test`, and
`birnam install` consume the publication through inert artifact inspection.
Only `check`, `build`, and `run --build` observe the complete source graph.

The command contract becomes:

~~~text
birnam check          observe and compile-check current sources
birnam build          observe sources and atomically publish artifacts
birnam run            execute the last successful publication
birnam run --build    build current sources, then execute that publication
birnam test           run authored tests against the last publication
birnam test --build   rebuild first
birnam install        install the last publication
birnam install --build rebuild first
~~~

Ordinary `run` no longer promises to diagnose source changes made after the
last build. That promise would require source observation in the launch path.
The publication is intentionally authoritative until another build succeeds.

## 3. Publication shape

A project publication remains under the target/profile directory and is still
published by staging and atomic rename:

~~~text
.birnam/build/<target>/<profile>/
  lib/
    application.dylib                       executable application root
    project-configuration.dylib             run-policy root
    project-modules.dylib                   test/install module root
    libbirnam___<project>Configuration.dylib canonical configuration link
    libbirnam___<project>Modules.dylib      canonical module-root link
    libbirnam_<module>.dylib          authored modules
    lib<native>.dylib                 wrapped native artifacts
~~~

`application.dylib` is an explicitly selected root, so its filename need not
encode its identity. A project without `main` has no application root and
cannot be run, but its module root may still support tests and installation.

### 3.1 Project module root

Build synthesizes one compiled Birnam module whose requirements are the
deduplicated, dependency-first resolved module set. It names declared wrapped
libraries through ordinary `Alien dlopen:` expressions, causing their native
requirements to be embedded in the module descriptor and included in preflight.

This root is activated for project tests without executing application entry
behavior. It is the installation root for library projects and remains separate
from the optimized executable application.

### 3.2 Configuration root

Build synthesizes a small module containing the manifest's run policy as
project-publication globals. Keeping this policy inside a compiled artifact
makes it available without reparsing the source manifest and lets `run` select
REPL support before application activation.

### 3.3 Application root

When the root manifest names `main`, build synthesizes an application root.
It requires the configuration root and contains one dependency-first compiled
program made from all resolved module bodies followed by the exact main source
bytes. Each bundled module's authored declaration is replaced by an equivalent
late-bound declaration inside the application program, and its loaded-state
effect is retained.
Wrapped native requirements are embedded directly in the application
descriptor. Activating this single root therefore avoids loading and linking
every project module separately while invoking main exactly once after the
complete native closure has passed preflight.

`ProjectArgs` is installed before application activation. Manifest-selected
REPL support modules are loaded after the configuration root exposes run policy
and before application activation, preserving the existing entry environment.

The application root's embedded source and revision digests cover both its
generated declaration and the captured main bytes. Main changes therefore
change the artifact itself rather than an external freshness record.

## 4. Build and reuse

Build observes and resolves sources exactly once. For each deterministic prior
module path it performs inert descriptor inspection:

- matching identity, ABI, target, source digest, revision digest, and ordered
  requirements permit reuse;
- any mismatch recompiles that module;
- a recompiled dependency makes its dependents dirty;
- missing or malformed prior artifacts are cache misses, not publication input;
- generated project and application roots use the same descriptor comparison;
- wrapped native build commands continue to run for every explicit build.

Reusable artifacts are copied into the private stage. New and reused artifacts
are preflighted from the completed stage before publication. A failed build
leaves the previous publication intact.

No artifact checksum is stored externally. Local project execution adopts the
same integrity seam as installed execution: inert descriptor validation,
complete-closure preflight, and platform artifact/signing integrity. Native
requirements are named inside the requiring descriptor rather than in a receipt.

## 5. Run

Ordinary run performs no project manifest parsing, source indexing, resolution,
or build-policy evaluation. It:

1. discovers the nearest project root by filesystem ascent;
2. selects `.birnam/build/<target>/<profile>/lib/application.dylib`;
3. inspects the application root and its complete sibling closure;
4. activates the configuration root from that preflighted closure;
5. reads its embedded run policy and applies command-line overrides;
6. installs `ProjectArgs`;
7. activates the application root exactly once; and
8. starts the selected REPL after main returns.

Missing application publication is a typed error directing the user to
`birnam build` or `birnam run --build`.

The run path must not parse `birnam.toml`, parse generated TOML, read source
files, invoke `ProjectPolicy`, or perform redundant closure preflight.

## 6. Test and install

`test` still reads test-source configuration because tests are authored source.
Each fresh worker activates the published project module root, installs the test
harness, and evaluates one test source. It never activates the application root.

`install` preflights both the module root and, when present, the application
root, then copies the union of their descriptor-discovered Birnam and native
closures. Explicit roots are installed under canonical identity-derived module
filenames. No source manifest or generated receipt is installed.

## 7. ProjectPolicy scope

`ProjectPolicy` continues to own provider selection, dependency closure,
diagnostics, wrapped-project ordering, and deterministic resolution. Receipt
construction, receipt validation, checksum comparison, and publication-record
methods are removed from its interface.

This makes the module deeper: callers receive a resolved source graph from one
policy interface, while publication inspection and activation remain behind the
compiled-artifact interface. The host retains filesystem, compiler-process,
native-build, staging, and rename adapters.

## 8. Compatibility and migration

Existing build directories containing only `artifacts.toml` and authored module
dylibs are not accepted as application publications. The diagnostic requests one
explicit rebuild. The next successful build atomically replaces the directory
and removes the obsolete receipt.

The change intentionally revises the stale-source behavior specified by Design
52 section 3.4. Designs 51, 52, 54, and 57 remain historical context but must be
annotated to point here wherever they describe a live receipt contract.

## 9. Acceptance

The implementation is complete when:

- build output contains no `artifacts.toml`;
- a second unchanged build reuses matching compiled module artifacts;
- missing or malformed prior artifacts rebuild safely;
- `run` succeeds after authored source and `birnam.toml` are made unreadable;
- `run --build` reflects changed main source;
- run invokes main exactly once and preserves argument/REPL overrides;
- tests activate project modules without invoking main;
- install copies the descriptor-discovered module/native closure;
- failed publication leaves the previous application runnable;
- project, Git-dependency, wrapped-native, relocation, and source-build gates pass;
  a future release additionally runs the immutable-archive Design 60H gate;
- no startup profile contains `parse_project_manifest`,
  `observe_project_snapshot`, or `ProjectPolicy` below ordinary run; and
- a reproducible project launch benchmark demonstrates improvement without
  regressing project command correctness. The original approximately twelve-second
  Digla observation is motivation, not the reproducible closeout baseline.

## 10. Closeout evidence — 2026-09-05

Ordinary run had retained two closure preflights; publication discovery now only
locates roots, and the consuming command preflights its selected closure once.
This also allows tests to run against the module root even if the application
artifact is malformed. Build output reports the number of reused modules.

Observed acceptance against a freshly built release runtime:

- `mise run verify`: Rust workspace tests, native prefix/lifecycle checks, and
  two independent source-only cold builds plus relocated execution passed.
- `BIRNAM_TEST_PREFIX=<prefix> mise run test`: **611 passed, 0 failed**.
- `BIRNAM_BIN=<prefix>/bin/birnam mise run rust-project-check-smoke`: passed.
  Covers missing publication; unchanged-module reuse; malformed/missing cache
  recovery; unavailable authored sources and unreadable manifest; stale main
  versus explicit rebuild; failed-build preservation and stage cleanup; test
  isolation from main and malformed application; main-once/argument/REPL override;
  transitive path and pinned Git/native dependencies; source-free project
  relocation and install; native preflight failure before main runs.
- `cargo fmt --all --check` and `git diff --check`: passed.

The first verification attempt was invalidated by an implementation edit during
the source-manifest guard. The completed gate was rerun after source settled.
No release tag, public release, or user-prefix installation was performed.

### Launch experiment

`scripts/project-launch-bench.py` constructs a private nine-module chain with a
pinned local Git dependency and a compiled C dylib. It validates output `42` for
every invocation. Fetch/build and one warmup are excluded; each measured launch
uses a fresh process, an unrelated cwd, and the same fixture shape. Source/runtime
identity is included in its JSON output. The experiment used the release runtime
and debug project profile on arm64 macOS 14.6, after the validation jobs completed.

| Runtime | Five warm launches (seconds) | Median |
| --- | --- | --- |
| Before, clean `572c1cf` source archive | 1.313239, 1.228479, 1.242032, 1.198348, 1.200436 | 1.228479 s |
| Design 65 plus closeout | 0.750448, 0.759271, 0.997970, 0.860605, 0.826210 | 0.826210 s |

This fixture's median launch improved **32.7% (1.49×)**. It is a diagnostic result
on a shared workstation, not an uncontended release benchmark or a measured
claim about Digla. A preliminary two-module run overlapped validation and was
excluded from this result.

Source-manifest SHA-256:

- before: `19de363fdabf62d511ff74a0cd69cfcd88b3beb0f5d45e8919af7ee69a9b7cf9`
- after: `2955a62643ef047406b57af1839a2f44bba104149223894d3c7080342d7406ab`

Reproduce using independently installed prefixes (the script modifies neither):

```sh
python3 scripts/project-launch-bench.py \
  --binary before=/absolute/before/bin/birnam \
  --binary after=/absolute/after/bin/birnam --runs 5 --modules 8
```

Raw logs and JSON for this session are under
`/tmp/birnam-publication-closeout.VJrOxG/`; they are temporary. The essential
measurements, identities, commands, and outcomes above are the durable record.
