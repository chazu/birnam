# Bulk traversal in the authored inventory prototype

Investigation at `7966f051`, 2026-10-31. The production Rust collector is unchanged.
This improves the earlier [method-source inventory prototype](../method-source-inventory-20261030/README.md);
it does not qualify that migration or fix general Integer arithmetic.

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

The existing inventory probe ran baseline/candidate/candidate/baseline against
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

## Evidence and remaining acceptance

[result.json](result.json) contains pooled medians, source/probe hashes and raw-file
hashes. Libraries are identified by the preceding arithmetic receipt, whose prefix
is unchanged. Raw results remain under
`.cargo-target/measurements/arithmetic-profile-20261031`.

The earlier experiment's [probe source](../method-source-inventory-20261030/probe.rs)
accepts the candidate file as its second argument. The final candidate runs used:

```sh
INVENTORY_COMPILE_MODE=compileSynchronous: .cargo-target/measurements/method-source-inventory-20261030/replay/probe/target/release/method-source-probe .cargo-target/measurements/image-refusal-20261031/final-prefix docs/measurements/inventory-bulk-20261031/inventory.birnam /tmp/inventory-bulk.json
```

The six fixtures exercise the current parser's CSTs. They do not prove equivalent
effects for replaced `children` versus `childAt:` operations, arbitrary substitute
CSTs, or the extreme depth boundary. Those cases and a fresh cold-bootstrap replay
remain required before production integration. The old bootstrap proof does not
automatically qualify this changed prototype. No runtime installation, production
caller switch, native code deletion or publication is claimed.

The result supports improving the algorithm alongside the runtime. It leaves
ordinary Integer performance, repeated control setup and migration acceptance open.
