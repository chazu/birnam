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

## Initial implementation timing acceptance — closed 2026-09-09

`birnam-epic-as7.1` and `birnam-epic-as7.20` closed on the host-gated record
[`as7-acceptance-comparison.json`](../measurements/as7-acceptance-comparison.json):
five focused runs of the unchanged `scripts/bench.sh` measurement functions
(before `f15b8fbe` twice, Store/PEG `3bff9357`, head `21cf6d7b`, fixed
`f24ff9b2`) plus the probe suite `bench/selfhost-probe.birnam` for repeated
Block creation, CodeObject reflection and traversal, method-dictionary and
inheritance lookup, and global import/rebinding. Macro, parse and relational
cases agree within 4% between the two before runs; single-process micro net
values do not (the same binary moved 20-80% on sub-microsecond cases), so the
micro verdicts below read raw per-iteration values.

| Case | before (two runs) | fixed `f24ff9b2` | Verdict |
| --- | ---: | ---: | --- |
| `blocks.create` raw ns/iter | 4753 / 4761 | 4477 | Accepted (as7.2): net 698/778 to 345 ns/op |
| `code.reflect-program` raw | 4753 / 4795 | 4135 | Accepted (as7.2): net 704/735 to 370 ns/op |
| `code.traverse-statements` net ns/op | 5718 / 5814 | 3744 | Accepted (as7.2): -34% |
| `lookup.dict256-first/last`, `lookup.inherit-depth16` raw | 5.3-5.7k | 5.2-5.4k | Neutral (as7.4): within same-binary drift; send/loop overhead dominates at 256 methods |
| `globals.import-512` raw | 5006 / 4391 | 4065 | Neutral (as7.3): the before binary disagrees with itself by more than the change |
| `globals.at-512`, `globals.rebind-512` | 5.5k-7.2k | 5.2k-6.6k | Neutral (as7.3) |
| `loops.whileTrue` ns/op | 30704 / 32009 | 25296 | Improved (-17 to -21%) |
| `docgen.full` | 39.6 / 39.1 s | 29.4 s | Accepted (-25.7%; RSS 321 MB flat) |
| `boot.stdlib-dylib` | 107.0 / 107.1 ms | 86.0 ms | Accepted after fix (-19.6%) |
| `boot.stdlib-source` | 270.7 / 269.5 ms | 241.5 ms | Accepted after fix (-10.7%) |

The head build carried the Design 71 startup regression plus ~4.5 ms from the
method-dictionary index: reverting `vtable.rs` alone restored 111.1 ms against
116.7 ms (15 interleaved reps). Every vtable record allocated and grew a hash
table, including the thousands holding a handful of methods at boot, with no
lookup benefit at that size. `f24ff9b2` scans the ordered keys for
dictionaries of at most 16 entries and builds the index only above that; the
as7.4 mutation, weak-sweep, inheritance and image tests pass unchanged.

Retained, not waived: the unchanged comparator exits 1 on before-to-fixed for
the single-process micro flags above and for `boot.bare` (+5.8% on three reps;
+3.7% same-binary drift; no difference over 25 interleaved reps). Between-build
layout variance on send-heavy micro cases remains ±15-20%; release claims stay
with the multi-process release gate.

Observed during diagnosis and filed separately: a stdlib dylib boot parses all
seven self-hosted floor sources through the language grammar twice (~250 KB per
boot), which is now the largest remaining startup cost.
