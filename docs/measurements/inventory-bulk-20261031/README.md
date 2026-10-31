# Bulk traversal in the authored inventory prototype

Investigation at `7966f051`, 2026-10-31. The production Rust collector is unchanged.
This improves the earlier [method-source inventory prototype](../method-source-inventory-20261030/README.md);
it does not qualify that migration or fix general Integer arithmetic.

Subsequent qualification repaired an early-initialization dependency and found a
stack overflow in synchronous compilation. Use ordinary **`compile:`** for this
prototype. The synchronous timings below remain diagnostic evidence, not a safe
production execution choice.

## Change

The earlier prototype manually walked every child using `childCount`, `childAt:`
and Integer index increments, with a second Integer increment for traversal depth.
The [new prototype](inventory.birnam) asks for the whole `children` collection and
folds it in order, removing the recursive child-index helper. A Word represents
the private traversal depth; the zero, one and 4096 limit are constructed once
when the prototype is installed. This uses existing operations and adds no native
primitive, service or representation.

Declaration recognition, source extraction and accumulation remain in Birnam.
The current node is considered before its children, and the fold threads the
accumulated results through children in order. The depth test occurs before visiting
a node. With the default Word operations, only depths below 4096 are incremented,
so the counter cannot wrap. Selector assembly and wrapper-unwrapping counters are
unchanged. Choosing Word for this private counter is not a language requirement
that users avoid Integers.

## Measurements

At `43d555f0`, the existing inventory probe ran baseline/candidate/candidate/baseline against
identical release libraries, using `compileSynchronous:` and normal automatic
collection. Each process checked all six fixtures against the native collector,
then took seven samples per fixture after two warmups. Builds and other tests did
not overlap the final timings.

| Workload | Earlier prototype | Bulk traversal and Word depth | Reduction |
| --- | ---: | ---: | ---: |
| 930-node dense fixture, 32 declarations | 372.56 ms | 71.61 ms | 80.8% |
| 2719-node source-registry module, 20 declarations | 918.92 ms | 374.69 ms | 59.2% |

All 24 fixture comparisons matched, including order, duplicates, nested definitions,
quoted non-definitions and syntactic recognition under a shadowed receiver name.
The intermediate bulk-only experiment measured roughly 306 ms and 526 ms for the
same two workloads; those single runs are exploratory, not the final comparison.

This remains too slow for the proposed cutover. Existing **complete metadata
analysis**, including parsing and other work, measured roughly 10 ms and 30 ms
respectively in the final runs. The authored prototype performs only the method
inventory part. Collection frequency also affects the inventory medians; the
reduction is not a general application-speed claim.

## Qualification follow-up

The initial bulk revision failed early bootstrap: its new top-level `let` bindings
were published by resumable compilation before `Globals` existed. The initializer
now executes inside a local Block. Constants remain captured by the installed
Methods, without publishing new globals or adding an early registry.

The [qualification probe](qualification.rs) and [runner](qualify.py) verify:

- Both compilation modes retain local initializer bindings and produce the expected
  nested declarations at starting depths 0, 4095, 4096 and 4097.
- An ordinary substitute object with CST messages works without PEG representation
  checks, preserves duplicate children and order, and survives precise collection.
- Replacing `children` changes subsequent traversal. Deliberately inconsistent
  `children` and `childAt:` implementations produce different inventories; this
  demonstrates the bulk protocol's behavior, not equivalence to indexed projection.
- Resumable compilation traverses actual chains of 4095 and 4096 ancestors and
  respects the cutoff. Synchronous compilation **overflows the native stack** on
  an actual 1024-ancestor chain, despite passing the direct counter-boundary tests.
- The [early-bootstrap patch](early-bootstrap.patch) compiles and executes the
  inventory while Globals, Error and SourceRegistry are absent, then bootstrap
  builds its three early modules from source. Only native libraries and the existing
  Lang seed are supplied; no Birnam module or image is supplied. This proves the
  exercised early route, not a full clean-source release build or production cutover.

A new run of the safe resumable mode matched all six fixtures. Its seven-sample
medians were **353 ms** for the dense fixture and **839 ms** for the real module,
against approximately 10 ms and 30 ms for complete native metadata analysis.
This is a diagnostic run, not a new matched speedup comparison. The performance
obstacle remains substantial.

Run the positive qualification checks on macOS with a fresh output directory:

```sh
python3 docs/measurements/inventory-bulk-20261031/qualify.py --prefix .cargo-target/measurements/image-refusal-20261031/final-prefix --output .cargo-target/measurements/inventory-bulk-recheck --early
```

The known synchronous stack-overflow reproduction is isolated from those positive
checks. Its command and failure log hash are recorded in
[qualification.json](qualification.json). The direct boundary tests alone must not
be used to claim safe deep traversal.

## Evidence and remaining acceptance

[result.json](result.json) retains the historical pooled synchronous medians and
the source hash from `43d555f0`, before the initializer repair. Libraries are
identified by the preceding arithmetic receipt, whose prefix is unchanged.
Raw timing results remain under
`.cargo-target/measurements/arithmetic-profile-20261031`.

The earlier experiment's [probe source](../method-source-inventory-20261030/probe.rs)
accepts the candidate file as its second argument. For a diagnostic timing run of
the current resumable prototype:

```sh
INVENTORY_COMPILE_MODE=compile: .cargo-target/measurements/method-source-inventory-20261030/replay/probe/target/release/method-source-probe .cargo-target/measurements/image-refusal-20261031/final-prefix docs/measurements/inventory-bulk-20261031/inventory.birnam /tmp/inventory-bulk.json
```

The checks cover selected substitute behavior and deep chains, not all malformed
CSTs, callback effects or production integration. A cutover must account for the
bulk-versus-indexed protocol distinction and the existing metadata pipeline's input
validation. No runtime installation, production caller switch, native code deletion
or publication is claimed.

The result supports improving the algorithm alongside the runtime. It leaves
ordinary Integer performance, repeated control setup and migration acceptance open.
