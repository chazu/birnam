# Self-hosting performance execution map

Epic: `birnam-epic-as7` — selected by the user on 2026-09-06.

Beads is authoritative for dependencies, acceptance and execution state. This
page maps the selected work to stable issue IDs; it is not a duplicate status
tracker. Run `br show <id>` for the complete contract and dependencies.

| Slice | Issue |
| --- | --- |
| Existing Store acceptance | `birnam-store-snapshot-plan-1nj` |
| Existing PEG acceptance | `birnam-peg-matching-plan-ki3` |
| baseline | `birnam-epic-as7.1` |
| program-metadata | `birnam-epic-as7.2` |
| global-index | `birnam-epic-as7.3` |
| method-index | `birnam-epic-as7.4` |
| sorting-design | `birnam-sorting-plan-byd` |
| map-design | `birnam-map-deletion-plan-iis` |
| stable-sort | `birnam-epic-as7.5` |
| string-order | `birnam-epic-as7.6` |
| map-delete | `birnam-epic-as7.7` |
| store-index-design | `birnam-epic-as7.8` |
| store-current-index | `birnam-epic-as7.9` |
| store-compaction | `birnam-epic-as7.10` |
| linux-design | `birnam-epic-as7.11` |
| linux-roots | `birnam-epic-as7.12` |
| linux-toolchain | `birnam-epic-as7.13` |
| linux-services | `birnam-epic-as7.14` |
| linux-bench | `birnam-epic-as7.15` |
| linux-acceptance | `birnam-epic-as7.16` |
| allocation-design | `birnam-epic-as7.17` |
| allocation-implementation | `birnam-epic-as7.18` |
| selfhost-gate | `birnam-epic-as7.19` |
| Initial host-gated timing acceptance | `birnam-epic-as7.20` |

## Execution and review boundary

The initial implementation queue consists of baseline acceptance, CodeProgram
metadata reuse, global indexing and method-dictionary indexing. The user
explicitly authorized continuing implementation while leaving timing acceptance
pending on 2026-09-06. Stories .2/.3/.4 therefore have no baseline blocker for
code/correctness work; .20 depends on those stories and baseline .1 and owns
their timing criteria. Work on the
immutable before-prefix remains reproducible throughout. Performance claims
require the unchanged host gates; correctness checks cannot substitute for them.

The first design review is [Design 73](../design/73-scalable-ordering-contract.md),
sorting and early-exit String ordering, extending Design 69 into an explicit
decision. Stop once its review document is ready.
Map deletion (Design 70), current Store indexing/compaction, Linux platform
boundaries and allocation/control representation retain separate design gates.
A design ticket closes only after an explicit accepted decision, not when its
proposal is written.

The Linux design selects CPU/libc support and covers ABI descriptors, native
roots, libtcc/ELF/PIC, artifact discovery, image materialization, foreign layouts
and benchmark adapters. Allocation implementation depends on both its accepted
design and Linux native-root proof. These changes must preserve the independent
Rust cold compiler and source-only bootstrap. This epic does not itself
translate the compiler or move bulk native kernels into Birnam.

Before accepting the next self-hosting migration, retain target-specific
workload, allocation, image, retirement and cold-bootstrap evidence. The final
migration-gate story depends on the implemented performance/portability spine;
Store follow-ups are selected or deferred by their own measured design.
