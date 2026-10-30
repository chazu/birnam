# Local Cells by default

2026-10-13. Implements the default-mutation choice in
[optional Store and Relational](optional-store-and-relational.md), following the
[owner-based reflection slice](../archive/2026-10-runtime-refactors/reflect-without-store.md).

**Ownership update, 2026-10-14:** [authored standalone Cells](../archive/2026-10-runtime-refactors/standalone-cells.md)
now replace the Store-owned local representation for new core Cells. This document
records the preceding default-API transition and its measurements.

## API transition

`[value asCell]` now returns a local Cell through the existing `db localCell:`
protocol. `value`, `set:`, `inc`, `dec`, shared aliases and independent Cell
construction retain their value behavior. These Cells create no facts and retain
no implicit history; `history` answers zero, even when the Store's `#value`
attribute has history enabled.

This deliberately changes the public default. Programs that need fact-backed
mutation use `[store cellFor: entity attr: attribute]` and select attribute history
and retention on that Store. Existing fact-backed handles retain their Store,
entity and attribute; the change does not convert them or purge old facts.
Image compatibility must preserve those existing objects and their aliases.

Compatibility checks also restore an image written by the previous fact-backed
`asCell` runtime. Its existing Cells, history and saved `asCell` definition survive
both mapped and rebuilt entry. Loading an old world does not silently replace its
saved method definitions with the new defaults.

The change also removes incidental fact publication by existing local-state
consumers such as String comparison, `Reflect factsOf:`, LSP accumulators and
ProjectPolicy's traversal Cells. It does not remove their intentional public
state, or change `factsOf:` to mean ordinary object slots.

Cell operations and construction remain late-bound sends. There is no optimizer
shortcut, deoptimization policy change, monkeypatch guard, new native layout or
new native state service. Local Cells still use Store's existing Cell implementation
and image representation. Separating that owner remains necessary before Store
can become an optional core dependency.

## Module traversal

`Module loadOrder:` now creates an invocation-local Cell with `localCell:`. Module
is bootstrapped before stdlib, so it cannot depend on `asCell` being installed.
Traversal order, deduplication and cycle errors retain their existing algorithm.

The previous fixed `Module/#topoOrder` fact shared a mutable order between nested
traversals. With local state, a collecting/reentrant `requiresOf:` implementation
can request another load order without overwriting the outer traversal. Existing
`#topoOrder` facts are left untouched; public registration, dependency and loading
facts remain in place.

## Evidence and limits

The [dedicated fixture](../../tests/fixtures/local-cells.birnam) checks local aliases,
nil, independent Cells, increment/decrement, zero fact and basis changes, explicit
Store history, collection, repeated String/fact-reflection consumers and nested
Module traversals. The [runner](../../scripts/local-cell-test.py) also saves shared
local Cells and explicit fact Cells, removes the source file, and exercises mapped
and rebuilt image entry.

`PrivateScratch.lean` now additionally models the deliberate choice between local
and fact handles. Its five new theorems cover default-write isolation, alias
updates, explicit fact publication, separation of local/fact identities and
independent default Cells. A negative mutation turns the default into a fact
handle and must fail. This is a model of the new value/state contract, not a proof
that old public fact/history behavior was preserved, or a proof of allocation,
GC, image encoding or replaced-method effects. Runtime checks cover those selected
paths. The existing publication and image protocols do not change in this slice.

The retention fixture compares 0, 1, 50 and 200 operation batches. Each has zero
fact growth and the same 21-object live delta from measurement-frame temporaries.
The zero-call control distinguishes that fixed overhead from accumulating state.

Image testing identified an existing limitation: independently created Store
instances are rejected by Image admission in both the baseline and candidate.
This slice does not change that behavior. Runtime tests cover independent Stores;
image tests use local Cells and explicit handles into the supported global Store.
Generic persistence of selected Store instances belongs in the later owner/image
work, including each instance's history and recovery policy.

## Qualification and measurements

[Source-bound receipt](../measurements/local-cell-defaults-20261013.json).
The rebuilt prefix regenerated authored bootstrap, tools and images using the
development Cargo cache. Production source files match the bundled sources;
the normal installed version was not replaced.

- 720 language checks passed, followed by the private-scratch, Reflect owner and
  code-inspector gates. The final dedicated Cell runner passed with the zero-call
  retention control; its initial fixed 16-object bound had incorrectly counted
  measurement temporaries as retained work.
- All 26 REPL golden checks passed, including image entry.
- Three native tests in the Store and closure-construction binaries passed,
  including captured Cells through waits, GC, resumption, cancellation and cleanup
  at both tested scheduling quanta. Their diagnostic prefix uses the unchanged
  TCC resource-metrics build.
- New and legacy Cells passed source-free mapped and rebuilt image entry. Old
  images retain the old `asCell` definition and explicit history; new images retain
  the local default. The native representation and image format are unchanged.
- The formal gate passed 223 Lean theorems and axiom auditing, rejected 48 Lean
  mutations, and obtained the expected outcomes from 123 TLC configurations.

The [paired runner](../../scripts/local-cell-bench.py) uses three fresh processes
per variant and workload, alternating baseline/candidate order. Twelve processes
ran sequentially after qualification, with no builds or profilers running.
Each operation warmed with ten iterations before timing; startup, preparation,
verification and post-batch explicit GC are excluded. Normal automatic GC remains
enabled. Both variants must produce independently checked sums.

| Workload | Baseline median CPU | Local-default median CPU | Reduction | Facts left by batch, before / after |
| --- | ---: | ---: | ---: | ---: |
| 1,000 Cell creations, updates and reads | 768.92 ms | 743.04 ms | 3.4% | 1,000 / 0 |
| 500 String ordering comparisons | 746.76 ms | 709.44 ms | 5.0% | 1,000 / 0 |

CPU ranges were 761.60–772.18 / 733.19–751.13 ms for Cells and
746.68–749.58 / 706.45–710.02 ms for String ordering. Post-GC process live-object
growth was 2,022 / 22 for Cells and 1,522 / 22 for String ordering in every run.
These counts include results and measurement temporaries; they are not byte or
whole-process memory measurements. The removed facts account for the substantial
lifetime improvement. CPU results describe these workloads and complete prefixes,
not an isolated getter or a whole-language speedup.

Reproduction:

```sh
python3 scripts/local-cell-test.py PREFIX --legacy-prefix OLD_PREFIX
python3 scripts/local-cell-bench.py OLD_PREFIX PREFIX
BIRNAM_TEST_PREFIX=PREFIX bash scripts/birnam-test.sh
python3 scripts/check-rooting-model.py
```
