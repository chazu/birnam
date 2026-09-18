# 81G — Integer as a module above Id

> **Status:** Executed Rust experiment, 2026-09-18, macOS arm64.
> Extends [81F](81f-isolated-id-seed-proof.md) with the smallest useful slice of
> Values. Both the seed implementation and its ABI source remain byte-for-byte
> unchanged. Production runtime extraction and Birnam primitive authoring remain
> separate work.

[81H](81h-storage-observation-and-tracing.md) extends this checkpoint with
observation/providers and a tracing module. This document's unchanged-seed claim
and counts describe the recorded 81G run; 81H changes seed methods/state while
preserving the original native API carrier.

## 1. Result and dependency

**Integer representation, construction and arithmetic work in a separately
loaded module using the existing seed API.** The module owns its payload layout,
default methods and representation checks. There is no Integer knowledge in Id,
no new SeedApi field, and no dependency on the private-vtable module.

The source is [integer/src/lib.rs](../../experiments/id-seed/integer/src/lib.rs).
Its only crate dependency is `birnam-seed-abi`. The compiled
`libbirnam_integer_experiment.dylib` is loaded after Id. Its initialization uses
`delegated`, `allocate`, `intern` and `addMethod` sends to create an Integer
behavior, a factory behavior/object, and their methods. The loader receives the
factory Oop explicitly; no global environment is needed to find it.

The full probe runs Integer tests **before** loading the object-model library.
A separate process runs from a directory containing only the probe, seed and
Integer binaries, and verifies those are its only mapped non-system images.
The full process then tests the two upper modules together by attaching private
behavior to one Integer. Neither module acquires a dependency on the other.

The five experimental crates are ABI, seed, Integer, object-model and probe.
All four implementation/test crates depend only on ABI. The main Birnam Cargo
workspace, installed host, production values and production IdApi are unchanged.

## 2. Native mounting and message protocol

Integer exports only these two native lifecycle functions:

```text
birnam_integer_create_v1(SeedApi*, Oop* factory_out) -> opaque module handle
birnam_integer_destroy_v1(opaque module handle)
```

The returned handle owns module state; the factory Oop is its message interface.
Destroy the seed before destroying this handle, and keep method code mapped until
all dictionaries and active invocations are gone. These exports introduce no
Integer declarations into the shared ABI crate. The probe declares the lifecycle
function types independently and resolves them with `dlsym`.

| Receiver | Selector | Operands | Default result |
| --- | --- | --- | --- |
| Factory | `integerBehavior` | None | Integer behavior Oop, available for ordinary method installation |
| Factory | `fromWord:` | One native Word | Fresh Integer whose signed payload uses those 64 bits |
| Integer | `asWord` | None | Native Word containing the signed payload's 64-bit representation |
| Integer | `+` | One Integer Oop | Fresh Integer containing the checked sum |
| Integer | `-` | One Integer Oop | Fresh Integer containing the checked difference |
| Integer | `*` | One Integer Oop | Fresh Integer containing the checked product |

The representation is one aligned `i64` in an Id-allocated payload. The native
Word carrier is already part of the experimental ABI; it is not another boxed
value family. This witness requires a 64-bit machine word. Default construction
does not canonicalize equal values: equal payloads can have distinct identities.

Arithmetic leaves both operands unchanged. It detects signed overflow before
result construction and returns module-specific status **`0x100`**. Overflow
neither wraps nor panics, in either build profile, and a send leaves its result
slot empty. Invalid arity, operand tag or representation returns the existing
`INVALID` status. Seed/overridden-method failure statuses propagate. These are
experimental native statuses, not a production Error/exception protocol or a
change to production Integer arithmetic.

Default arithmetic reads its own operand representation directly; it does not
define coercion or promise to call `asWord` as a conversion hook. After checking
the operation it **sends `fromWord:` to the factory**. The default constructor
in turn **sends `allocate` to Integer behavior**. Both construction and allocation
therefore remain replaceable, as do the arithmetic methods themselves. No
private cached constructor entry bypasses those sends.

## 3. Representation authority belongs to the module

Behavior identity is not proof of payload shape. The seed permits reassociation
with arbitrary behavior, so checking an object's current behavior alone would
permit reading an unrelated payload as an integer.

Integer therefore maintains its own set of the identities its constructor has
initialized. This set records **provenance, not numeric contents**: numbers live
only in their object payloads. Default arithmetic/accessors validate membership
before reading. Giving a real Integer private behavior preserves its payload
authority; attaching Integer behavior to an unrelated object does not grant it.
An Integer from another module instance is outside this representation domain.

The default allocator supplies fresh writable storage of the requested extent
and alignment. Custom allocators must honor that protocol. The constructor
checks the returned tag, live behavior association, alignment and duplicate
identity within its own set. It cannot validate every arbitrary allocator's
storage claims: it does not inspect a seed-private extent registry. Native
methods remain trusted code under 81F's calling contract.

No module-state borrow spans the allocation send. Method/state lifetimes are
pinned through seed shutdown, including when a constructor or allocator is
replaced. Failed publication aborts because an overridden installer could publish
before returning failure, and there is not yet a retirement/rollback protocol.

The provenance set is a bounded-experiment choice under retained storage. A
collector integration must define roots and retirement of these entries before
addresses can be reused; it must not accidentally make every Integer permanently
live. This is an upper-module lifetime obligation, not a reason to move numeric
representation into Id. No performance claim is made for these checks.

## 4. Executed acceptance

Run the extended experiment on macOS:

```sh
mise run id-seed-experiment
```

The [driver](../../scripts/id-seed-experiment.py) builds a fresh source snapshot
offline and retains the original 81F checks. It additionally verifies the seed
and ABI SHA-256 values against 81F's successful run, audits the Integer dependency
closure and two exports, and checks both complete and Integer-only process image
inventories. Source/binary hashes and command logs are recorded in the receipt.

The [independent probe](../../experiments/id-seed/probe/src/integer.rs) does not
link the Integer implementation or read its private state/payload directly.

| New case | Required observation |
| --- | --- |
| Construction/read | Zero, positive/negative values and both signed limits round-trip; repeated construction has distinct identity |
| Checked arithmetic | 243 cases from nine boundary/interior values and three operations agree with an independent `i128` oracle; both inputs retain their values |
| Overflow before construction | Six explicit overflow cases return `0x100`, leave the result empty, and do not invoke the wrapped result constructor |
| Invalid operands | Wrong arity/tags, unknown identities and objects merely reassociated with Integer behavior are rejected before payload access |
| Arithmetic replacement | Replacing `+` returns the right operand's identity; restoring it returns a fresh summed value |
| Constructor/allocation replacement | Arithmetic invokes both wrappers; explicit construction does too, with exact call counts |
| Independent seeds | Separate Integer instances operate independently and reject each other's operands; destroying one leaves the other working |
| Private-behavior composition | One Integer gets its own `+`; its peer retains addition, inherited subtraction still works, and payload access survives reassociation |

**Result: 19 full-process cases and 11 Integer-only cases pass in both debug and
release.** Rustfmt, Clippy with warnings denied, source-boundary hashes and
dependency/export/mapped-image audits pass. The full count includes all original
81F cases; the Integer-only count includes three seed baseline cases, seven
Integer cases and its own ordered shutdown.

All three negative controls are detected by the unchanged probe: the two
original seed mutations from 81F, and an Integer mutation replacing checked
addition with wrapping addition. The latter fails the independent arithmetic
oracle after construction/read passes. Broken variants live only in disposable
build snapshots.

The successful local receipt is
`.cargo-target/id-seed-experiment/20260918-104051-iyx_8oq7/receipt.json`.
It records `c8c4da5a` as the Git baseline plus exact hashes of the experimental
sources; that documentation commit alone does not contain either experiment.

## 5. Consequence for the sequence

This establishes a bounded Integer slice of 81E's L2 through seed semantics plus
access to the module's own representation. It provides a useful future target
for Birnam primitive authoring: preserve these observable operations and dynamic
calls while replacing their implementation language.

The result does not implement general Values/collections, automatic reclamation,
runtime code unloading, production extraction or Birnam compilation. The normal
production regression/release gates were not run for this isolated opt-in
workspace; they remain applicable when production runtime or ABI code changes.

Integer is the smaller addition selected before 81F's collector proof. The next
major unresolved integration boundary remains I3/I4: mount collection over the
existing seed storage, describe owned payloads and roots, and retire side indexes
before address reuse. Callable ownership and the Birnam authoring path remain
separate acceptance obligations.
