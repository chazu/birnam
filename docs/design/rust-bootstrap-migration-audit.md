# Remaining Rust modules and cold bootstrap

Audit date: 2026-10-10. Baseline: `c072e3ac`, before the Trace port.

The recommended next rewrites are Trace, Relational, ProjectBuild and HostProject.
They contain behavior that can live in Birnam without replacing the compiler's
cold-bootstrap seed. Compiler, value and execution implementations need a
different treatment: production behavior can move to Birnam, but their bootstrap
implementations cannot yet disappear.

Retain Rust genuinely required to build and start from a clean checkout. Track
authored production implementations separately from retained bootstrap seeds.
Renaming surplus native code does not complete a port.

## Inventory and measurement

The baseline has 51 workspace crates: 39 production native libraries, one
build-only Lang seed, and 11 support/tooling crates. There are 90,795 physical
Rust lines under their `src/` directories, including comments and embedded unit
tests, excluding separate integration tests. Native crates account for 69,285
lines. These are scale indicators, not exact removable production-code counts.

There are also 43 non-test Birnam files under `modules/`, plus 13 test/runner
files. Of the 43, 42 declare named modules; `error.birnam` is bootstrap policy.
Two additional Birnam wrappers live under `wrapped/`.

The important distinction is needed before the first Birnam artifacts exist
versus currently loaded unconditionally. [Bootstrap](../../rust/birnam-bootstrap/src/lib.rs)
and the [Host loader](../../rust/birnam-host/src/runtime.rs) currently load more
than the first-artifact compiler needs. Removing a native implementation requires
changing startup wiring as well as rewriting its behavior.

The bootstrap classifications below are source/dependency assessments. They are
not experimental proofs obtained by removing each module and rebuilding.

## Clearest migration candidates

| Module | Rust lines | Complexity | Bootstrap assessment and migration boundary |
| --- | ---: | --- | --- |
| Trace | 443 | Low to medium | Diagnostic graph validation and reachability over Storage observations. No apparent first-artifact dependency. Strongest small complete port. |
| Relational | 3,043 | Medium to high | Queries, joins, aggregates, pull, temporal queries and rule evaluation. The compiler needs Store facts, but apparently not this query engine. Strong complete-port candidate. |
| ProjectBuild | 2,100 | Medium to high | Compilation sequencing, reuse checks, generated publications, staging and installation. The Birnam installer has its own driver. Move policy while retaining filesystem/process primitives. |
| HostProject | 1,458 | Medium | Project discovery, source observations, dependencies and editor context. No fundamental first-artifact role. Can follow ProjectBuild or be ported independently. |
| Storage | 386 | Medium to high | Diagnostic allocation/root/edge snapshots, not ordinary collection or compilation. Observation must not accidentally change the graph being observed. |
| HostInteraction | 297 | Medium to high | REPL/socket/tool orchestration. Mostly portable, but its idle host boundary is significant; a direct translation of its loop is unsafe. |

The first four account for 7,044 lines, with another 1,973 lines in shared
`project-support` potentially affected. This roughly 9,000-line area is not a
promise that every line can disappear.

Trace has a useful boundary: Storage constructs its snapshot before delivering
allocation/root/edge callbacks. Trace validates identities, computes reachability
and reports results. Its graph algorithm is ordinary collection processing.
Preserve gap handling, deterministic ordering, callback failures, reentrancy
rejection and cleanup.

Relational adds query methods to Store. Compiler metadata uses Store's
`currentValue:attr:` and `assert:attr:value:` directly. Store publishes `db`
itself, so removing Relational does not inherently remove that binding. An
authored replacement must supply `LogicVar` before programs using logic-variable
literals execute.

ProjectBuild already delegates decisions to authored ProjectPolicy. Remaining
Rust largely assembles observations, runs compilation and constructs publications.
Preserve atomic publication, digest/reuse validation and explicit dependency
fetching.

HostInteraction is not merely a small loop. Generation requires
`Execution retirementReady`, which checks that no execution is entered. An
enclosing Birnam loop changes that condition. Keep a minimal native host driver
until the authored interaction protocol explicitly returns to the host for
generation changes. See [running entry](../../rust/birnam-generation/src/candidate_phase.rs).

## Current cold compiler and bootstrap dependencies

| Module | Rust lines | Complexity | What should remain for now |
| --- | ---: | --- | --- |
| lang-seed | 822 | Low to medium | Grammar seed, already build-only and removed from the installed prefix. |
| peg | 1,445 | Medium | Matching and CST construction used by the grammar seed. |
| codegen-runtime | 3,248 | High | Source compilation, metadata, artifact admission and compiler orchestration. |
| native-backend | 1,738 | High | Portable-program lowering and C emission; also depends heavily on shared codegen. |
| artifact | 9,285 | High | Canonical program/artifact/selection/frame codecs and admission. Much is shared codec code, not just its native receiver. |
| generation | 4,477 | Very high | Compiler source registry plus publication, rollback, compensation and recovery. The current compiler uses its registry and authorization operations. |
| store | 2,646 | High | Facts, Globals, Cells, metadata storage and mutation hooks. Keep separate from Relational's query algorithms. |
| host-entry | 453 | Medium | Current builds invoke its compilation/script entry paths. Image/worker/application policy can be separated from that seed entry. |
| string | 1,104 | Medium | Text constructors and operations used by grammar/compiler/reflection startup. |
| vector | 845 | Medium | Persistent collections used by compiler and bootstrap interfaces. |
| map | 908 | Medium | Persistent map behavior used by the language/compiler environment. |
| integer | 1,148 | Medium | Representation, constructors and native seed operations. Production checked arithmetic is already authored. |
| boolean | 356 | Low | Canonical identities and bootstrap behavior. Production selection policy is already authored. |
| nil | 207 | Low | Canonical nil identity and early behavior. |
| float | 619 | Medium | Numeric representation, literals, conversions and mixed arithmetic. |

Required for bootstrap does not justify retaining every method forever. It means
deleting the whole implementation now breaks the current dependency chain. For
Generation, a source registry is needed early; that does not establish that all
live-generation transaction policy must remain Rust.

String/Vector/Map are reasonable future production ports, but less attractive
under the current deletion priority: their native seeds would still be needed.
The [value foundation](native-value-foundation.md#bootstrap-progress) records
the remaining compiler/reflection dependency.

## Native runtime floor

| Module | Rust lines | Complexity | Assessment |
| --- | ---: | --- | --- |
| id | 3,201 | High | Dispatch, identity, allocation records, symbols and native loading. |
| root-registry | 546 | Medium | Lifetime protection before authored execution exists. |
| namespace | 629 | Low to medium | Publication and lookup of early completed objects. |
| bootstrap | 498 | Medium | Initial construction and native loading. Early prefix remains necessary; later sequencing can move. |
| collect | 1,398 | High | Collection, marking, sweeping and native-stack interaction. |
| object-model | 1,171 | High | Allocation, reference slots, cloning and representation operations. |
| word | 776 | Medium | Primitive arithmetic/bit operations supporting authored values. |
| bytes | 1,238 | Medium | Owned byte storage and primitive access/framing operations. |
| execution | 13,060 | Very high | Methods, Blocks, frames, continuations, scheduling, native ABI and recovery. |
| portable-code | 1,824 | High | CodeObject/program representations and lifetime used by generated execution. Higher-level manipulation can eventually move. |
| tcc | 569 | High boundary complexity | Native compilation and executable mapping ownership. |
| prelude | 152 | Low | Early System identity construction; little substantive policy to rewrite. |

These contain potential internal reductions, but size alone does not make them
good wholesale migration targets. Execution covers much more than the minimum
needed to invoke a compiled method.

## External effects and later startup

| Module | Rust lines | Cold-bootstrap relevance | Migration assessment |
| --- | ---: | --- | --- |
| system-input | 192 | Interactive input is not needed for first-artifact compilation. | Small port through foreign-call facilities; preserve EOF/error behavior. |
| environment | 211 | Language-level environment access is not inherently needed by the compiler seed. | Small possible port; mostly an OS boundary. |
| system-output | 256 | Used by current build scripts and diagnostics. | Move formatting/policy; preserve a bootstrap output path. |
| file-io | 433 | Current seed scripts read source through it. | Keep necessary bootstrap I/O; higher-level path/error policy can move. |
| alien | 1,166 | Not inherently needed for first artifacts; needed by later foreign-library use. | Wrapper generation is already authored. Address ownership, loading and native calls require a native boundary. |
| image | 2,027 | Not required before first artifacts, but installation builds startup images. | ImageFormat is already authored. Heap/code restoration and owner coordination make this a high-risk port. |
| isolate | 2,910 | Workers are not needed to bootstrap the compiler. | Protocol encoding and lifecycle policy can move; process/IPC/thread machinery needs native facilities. |

Not being a compiler-seed dependency does not make complete Rust deletion
immediately straightforward. Replacements must reach necessary external effects
through existing native owners or foreign calls.

## Support and tooling crates

| Crate | Rust lines | Migration relevance |
| --- | ---: | --- |
| codegen | 11,083 | Shared lowering, portable semantics, optimization and generation planning. Include it in compiler rewrite estimates. |
| project-support | 1,973 | Shared project records, validation, value conversion and Git support. |
| host | 3,070 | Necessary launch entry mixed with migratable orchestration. |
| host-support | 848 | Shared host interfaces, lifetime scopes and value handling. |
| cli | 1,454 | Command parsing and dispatch, including cold-build entry. |
| build | 1,105 | Independent non-Birnam build/install driver. |
| abi | 638 | Native contracts. |
| siphon | 77 | Initial native loading. |
| native-compiler | 440 | Native compiler integration. |
| artifact-codec | 12 | Reuses Artifact's implementation; apparent size is misleading. |
| image-codec | 810 | Shared image codec/contract support. |

## Recommended sequence and acceptance

1. Port Trace completely, preserving Storage as the snapshot owner.
2. Port Relational, keeping Store unchanged and admitting the authored query layer
   before its first consumers.
3. Port ProjectBuild, using HostProject observations and authored ProjectPolicy.
4. Port HostProject and remove displaced project-support code.
5. Reassess Storage and input/environment wrappers. Handle HostInteraction only
   with an explicit idle-boundary contract.

For each complete port, run a fresh cold installation with the displaced Rust
implementation absent, then installed startup and affected behavior/image checks.
Removing a module only from ordinary startup is insufficient. Keep independent
semantic oracles, current-message dispatch, callback/reentrancy/GC checks and
exact native failure/result behavior. Record focused tests, installation, broader
validation and delivery as separate evidence states.
