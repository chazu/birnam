# 81F — Standalone Id seed and object-model boundary proof

> **Status:** Executed Rust experiment, 2026-09-18, macOS arm64.
> The bounded witness in [81E §6](81e-seed-sufficiency-audit.md#6-what-this-audit-settles-and-the-next-bounded-proof)
> passes in debug and release. This is a new isolated implementation, not an
> extraction of production `birnam-id`, a replacement installed runtime, or a
> Birnam primitive-authoring proof.
> For the production outcome through 81CA and current remaining work, see the
> [phase closeout](81-production-extraction-status.md). Historical next-step
> recommendations below do not constitute the current work queue.

[81G](81g-integer-module-proof.md) extends the executable experiment with an
independently loaded Integer module and leaves the seed/ABI sources unchanged.
The artifact inventory and counts below describe the original 81F run; the
current driver also runs the 81G cases and its Integer-only configuration.
[81H](81h-storage-observation-and-tracing.md) subsequently adds storage
observation and tracing, retaining the original native API carrier. Its separate
protocol and phase guards intentionally change the experimental seed.

## 1. Result

**Private-vtable policy can live above Id.** A separately loaded Rust library
creates/reuses private behavior, attaches it to an existing object, and installs
methods without a seed-owned private-vtable owner registry or access to a Rust
Runtime/Heap/VtableRecord. It uses messages plus declared behavior read and
reassociation operations.

The seed preserves three important forms of malleability in the exercised paths:

- Default lookup sends `lookup` to its parent. An ordinary object with a custom
  payload and resolver can be that parent, or be attached directly as behavior.
- The metacircular termination case reads the **current** root lookup binding.
  Replacing that method affects even direct sends to the recursion root.
- Method replacement during an active call takes effect for a nested send;
  the active invocation can finish using its old method. No seed state borrow
  crosses a call into consumer code.

This supports a concrete first boundary. It does **not** prove that the original
five methods are sufficient for all upper layers, nor that the experimental
native table is the smallest possible table.

## 2. Artifacts, construction order and state ownership

Sources live in [experiments/id-seed](../../experiments/id-seed/README.md), a
separate Cargo workspace. Each implementation/test crate depends only on the
small ABI crate; the ABI crate has no dependencies.

| Artifact | Loaded when | Implementation and owned state |
| --- | --- | --- |
| `birnam-seed-probe` | Process entry | Rust test host; loader, assertions and test method bodies/data; statically includes ABI helpers only from this workspace |
| `libbirnam_seed_experiment.dylib` | First explicit `dlopen` | Rust Id implementation; allocations/behavior headers, Symbol index, default dictionaries/parents, core roots and active-call depth |
| `libbirnam_object_model_experiment.dylib` | After the seed has constructed its roots and passed basic sends | Rust upper library; private-vtable owner/reuse map and diagnostic count |

The seed creates the self-associated Vtable root, Object/Symbol behaviors, a
Symbol interning receiver and selectors. It directly installs the initial
lookup/addMethod bindings needed to break bootstrap recursion, then installs
remaining methods through ordinary sends. The object-model library receives
the seed handle, interns its own selector and publishes `ensurePrivateVtable`
on Object through `addMethod`.

At shutdown the seed is destroyed first, retiring dictionaries referencing
consumer method data. Consumer state is destroyed next, then libraries are
unmapped. A destroy attempt from inside an active method is rejected. Test
method state also remains alive through seed destruction.

These are experimental artifacts, not stage 0/stage 1 installer products.
Nothing in the installed system is replaced. The test executable contains native
fixture methods; it is not the proposed platform-only siphon. Rust's standard
library/allocator supplies implementation machinery in the experiment; this
does not establish the eventual Birnam-to-platform construction path.

## 3. Declared experimental boundary

The [ABI carrier](../../experiments/id-seed/abi/src/lib.rs) contains:

- Version/size, an opaque seed context, core roots and selector handles.
- `send`, `behavior_of` and `reassociate` native entries.
- A native method entry/data pair and explicit native operand tags for an Oop,
  machine word, borrowed bytes, method pair or absence. These are not a boxed
  Values layer or any of the production Birnam value encodings.
- Separate create/destroy exports for seed and upper library. The upper
  library's returned handle includes its selector and a test diagnostic counter.

The five starting methods remain messages: `intern`, `addMethod`, `lookup`,
`allocate`, and `delegated`. Default behaviors additionally expose method
removal and parent read/write as messages. The seed is authoritative for those
facts; the upper library never decodes a seed header or dictionary.

Behavior reassociation preserves the object's payload address. The assigned
behavior must be a live object in that seed, but need not have the default
dictionary representation. Default parent mutation has the same membership
rule. Private-vtable policy expects a meaningful `delegated` operation; custom
behavior is not required to adopt that policy.

Association is exposed directly here to obtain a small, testable I1 contract
that works independently of an object's chosen lookup. A future protocol could
instead expose the same authority through a designated service object. This
experiment does not settle that carrier choice, freeze these signatures, or
justify retaining the production 54-field table.

Native callers are trusted and owner-thread-only. Borrowed operands last for
the call; payload access is limited to the caller's own declared layout, with
word alignment. No Rust implementation types cross the ABI. Storage is retained
until shutdown, bounded by 8,192 allocations, 64 KiB per payload and 4,096 bytes
per interned name. Sends accept at most 16 operands and have a depth limit of
128. Symbols are strongly retained in this proof. Arbitrary pointer validation,
language exceptions and transactional publication are outside its contract;
failed construction/publication can abort.

## 4. Executed evidence

Reproduce on macOS with:

```sh
mise run id-seed-experiment
```

The [driver](../../scripts/id-seed-experiment.py) copies only the experiment's
Rust sources/manifests/lockfile into a fresh directory, builds offline, and copies
the three artifacts into an isolated execution directory. It verifies the Cargo
graph, runs Rustfmt/Clippy, checks Mach-O dependencies and exports, rejects Birnam
undefined imports, and inspects dyld's actual mapped-image inventory. The only
mapped non-system images must be the three exact experimental artifact paths.
The dylib's own Mach-O identity is recorded separately from load dependencies.

No production Birnam crate occurs in the source dependency closure. No
production runtime/library appears in the linked dependencies or observed
mapped images. This establishes the exercised witness's isolation; it is not
a security sandbox or evidence about the production installed binary.

| Probe case | Required observation |
| --- | --- |
| Bootstrap and payload | Self-associated Vtable, expected roots, unique identities, writable declared payload |
| Symbol identity | Repeated byte strings intern identically, including empty and non-UTF-8 input |
| Binding/parent mutation | Override, remove, inherited fallback, missing send, and changed parent behave as expected |
| Upper private behavior | One object's override leaves its peer unchanged; repeated request reuses behavior; external reassociation invalidates reuse |
| Custom parent lookup | Ordinary resolver payload selects alternating methods; all four lookup effects occur, including direct behavior attachment |
| Reentrant replacement | Old method returns 11; nested and later sends return 22; destruction while active is refused |
| Root lookup replacement | A direct send to the recursion root invokes the replacement; restoring it stops wrapper effects |
| Allocation replacement | A custom metabehavior's allocation wrapper is called and delegates to the original implementation |
| Invalid inputs and cycles | Selected invalid inputs are rejected; cyclic delegation reaches the bound; repairing it restores healthy sends |
| Two independent seeds | Foreign identities are rejected; destruction of one leaves the other working |
| Ordered shutdown | Seed and consumer state retire before their code is unmapped |

**Result: all 11 cases pass in both debug and release.** Rustfmt and Clippy
with warnings denied pass. Linked and observed runtime dependency audits pass.

Two deliberately broken seeds are built in disposable copies. The unchanged
debug probe and upper library must reject them at the specified case, after
every preceding case passes:

| Mutation | Observed rejection |
| --- | --- |
| Replace parent message sending with a raw default-dictionary walk | Custom-parent case returns `INVALID` instead of 11 |
| Use the original lookup implementation at the recursion root | Root-replacement case observes zero wrapper invocations instead of one |

Both controls are detected. They establish that ordinary dictionary behavior
alone cannot satisfy the oracle, and that merely having a replaceable binding
somewhere in the dispatch chain is insufficient.

The local run receipt, command logs, source hashes and binary hashes are retained
under `.cargo-target/id-seed-experiment/`. The recorded successful run is
`20260918-100341-mgyf_i33/receipt.json`. Its source baseline is documentation commit `c8c4da5a` plus
the source hashes in the receipt; that commit alone does not contain this
experiment. Raw build/measurement artifacts remain outside tracked docs.

## 5. What remains unproved

| 81E obligation | Evidence supplied here | Remaining work |
| --- | --- | --- |
| I1 — Association | Behavior read/reassociation and external private-vtable policy | Collector roots, mutation participation and production migration |
| I2 — Seed state | Default parent/method protocols and custom behavior distinction | Symbol-byte borrowing, direct-binding reflection, complete late-mount inventory |
| I3/I4 — Storage/collection | Bounded construction storage owned by one seed | Traversal/edges, safepoints, provider attachment, reclamation/index retirement and address reuse |
| I5 — Callable lifetime | Reentry with process-pinned code/data; active-call destruction guard | Acquired ownership before effectful lookup, retirement, unloading and resumable invocation |
| I6 — Mutation visibility | Later uncached sends see mutations; effectful lookup runs each time | Observer registration/order, Store mirroring, validated cache assumptions |
| L — Birnam primitive authoring | No evidence: methods are Rust | Compile primitive bodies with no runtime dependency on stage-0 services |

There is no GC, Machine, Frame, compiler, image reader, optimizer or mutation
observer in the seed. Avoiding caches gives a clear semantic reference but says
nothing about speed near C. No timing comparison or speed claim is made.

The experiment does not establish equivalence to every production Id behavior
or move production source into layers. Existing full-runtime regression and
release gates were not run for this separate opt-in workspace; those gates
remain required when the production runtime/ABI changes.

The next major integration proof should define I3/I4 and mount a separately owned collector
over the **existing** seed allocation domain. Exercise root transfer, core
behavior/Symbol edges, reclamation and address reuse without exporting Rust heap
types. Follow with I5's acquired-callable/retirement witness. Keep the Birnam
primitive-authoring proof explicit and separate from these Rust interface tests.
The smaller Integer module in 81G was selected and executed before that collector
work; it remains within this experiment's retained-storage lifetime.
