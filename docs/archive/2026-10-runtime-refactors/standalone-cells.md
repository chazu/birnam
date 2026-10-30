# Authored Cells independent of Store storage

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-14. Follow-up to [local Cells by default](../../design/local-cells-by-default.md).

**2026-10-20 update:** the completion slice removes Store's duplicate native
`localCell:` / `localCell:prototype:` implementation and tagged-local restoration.
Only explicit fact-backed handles use `CellVt`. Remaining examples and fixtures
use the authored owner. Old images containing native local Cells must be rebuilt;
the old compatibility runner options have been removed. The earlier qualification
and measurements below describe the original migration, not current compatibility.

The current absence runner loads the actual compiled Cell initializer, including
its captured receiver/index values; the original exported-zero-capture method
assembly described below was replaced during the socket/Cell accessor slice.
Globals and Method publication are now independent of Store. Full Store-free
Image and production composition are still pending in the
[completion plan](store-independent-core-completion.md).

## Delivered boundary

`Cell` is an ordinary Birnam prototype with one ObjectModel reference slot.
Its implementation is [cell.birnam](../../../modules/cell.birnam): `with:`, `value`,
`set:`, `history`, `inc` and `dec` are authored methods. Construction clones the
receiver and sends the clone its current `set:` method. `asCell` sends the current
`Cell with:`; it no longer asks `db` for local state.

The factory is published in Globals only after its methods are installed. No
new native Cell registry, payload access, service table or image family was added.
Cloning uses ordinary shallow-object semantics: the container has a new identity,
its value is initially shared, and replacing that value does not modify the source
Cell. Aliases to one Cell continue to share updates. History remains zero.

Module traversal, Markdown, NativeDoc, AlienCompiler and ImageFormat now construct
their private state through this owner. ImageFormat clones its own worker/Cell
prototype, so its helpers do not require a fresh global `Cell` binding after
restoring a legacy world. There are no remaining `db localCell:` calls in the
production authored modules.

Store retains its existing fact Cells and legacy local-Cell representation/API
for explicit callers and old images. Those old objects are not converted into
ordinary-slot Cells, and Store does not track new Cell instances. This compatibility
boundary adds no Rust implementation of the new Cell policy.

Extensions intended for new local Cells should target `Cell`, not the legacy
Store-owned `CellVt`. A derived ordinary Cell constructs another instance with
`with:`. `Store localCell:prototype:` continues to accept its own legacy Cell
prototypes; it is not a constructor for the new ordinary representation.

## Bootstrap and publication

The build produces a `cell` artifact before `modules`; production Bootstrap loads
it before the Module manager and Lang. Module registers the already-loaded Cell
module once its own registration protocol exists. That registration is an
ordinary send through a manager binding, not a second source declaration: the
Module manager source and compiled artifact retain the identity `#modules`. This preserves early traversal
availability without relying on stdlib's later `asCell` definition.

The complete language bootstrap still loads Store: Globals, compiler metadata and
method publication have not yet moved. Cell's initialization uses those current
binding/publication interfaces. Removing that wider dependency is separate from
making Cell storage and method execution independent of a database.

Cell uses the normal resumable compilation path. A tested synchronous build
introduced a native suspension barrier when a replacement setter waited inside
`with:`; that mode was rejected. A replacement setter can now wait, survive GC and
resume construction. This preserves ordinary send behavior rather than introducing
a frozen setter or a special admission check.

## Store absent test

[The runner](../../../scripts/cell-without-store-test.py) exports canonical CodeObjects
from the installed Cell methods and checks their capture counts. It then builds
a minimal test floor containing no Store, Relational, Bootstrap, Lang or Generation
library. The [native harness](../../../rust/birnam-siphon/tests/cell_without_store.rs)
uses ObjectModel to construct the one-slot prototype and the existing compiler /
Execution interfaces to materialize those actual method bodies. It does not
reimplement Cell policy in Rust.

Namespace serves as the binding owner for this floor, including the compiler's
existing `Globals` receiver convention. Bootstrap dictionary installation uses
the existing raw publication mechanism. No fake Store, fact API or alternative
native service table is supplied. Tests check creation, updates, aliases, nil,
arithmetic helpers, clone independence, value reachability and reclamation.

This establishes Store-free Cell operations, not Store-free activation of every
ordinary module or a full Store-free language image. The test is included after
the normal native integration gate.

## Persistence and verification

New Cells use existing ordinary-object slots, private vtables and authored method
restoration. No `CellVt` native family is introduced for them; the existing
Store-owned `CellVt` remains the legacy family. New and old representations can
coexist without a codec or image-version change.

The [Cell owner fixture](../../../tests/fixtures/cell-owner.birnam) checks cloning,
collecting setter/factory replacement, suspension and resumption, and core clients
after replacing Store's local-Cell constructor with a rejecting implementation.
The existing retention, closure, code-inspector and image checks remain applicable.

Three new Lean theorems model one-slot reads/writes and shallow container cloning,
building on the earlier alias/publication model. The wrong-slot mutation must be
rejected. This is a value/identity abstraction, not a proof of the compiler, native
storage, scheduling or image codec; runtime tests exercise those boundaries.

## Qualification and cost

[Source-bound receipt](../../measurements/standalone-cells-20261014.json).
The final prefix regenerated the authored bootstrap, tools and startup images
using the development Cargo cache. This is not clean-cache release qualification;
the normal installed version was not changed.

Passed: 720 language checks and the scratch/Reflect/inspector/Cell gates; 26 REPL
goldens; the six-method Store-absent test; constructor replacement with suspension
and GC; source-free mapped/rebuilt images from both legacy fact-backed and legacy
local-Cell runtimes; Lang's seed comparison/self-compilation checks; and
AlienCompiler's 64 exact-output and eight ABI cases without stdlib/source loading.
The relocated Host/CLI acceptance also passed on a separate verified prefix copy.
Formal verification checked 226 Lean theorems with standard axiom auditing,
rejected 49 Lean mutations and obtained all 123 expected TLC outcomes.

There is a performance regression relative to the previous Store-owned local
Cells. Three fresh processes per variant and workload ran sequentially after
qualification, alternating baseline/candidate order. The existing runner's
`--baseline-local` option verifies that neither representation creates facts.

| Workload | Native local baseline | Authored Cell | CPU increase |
| --- | ---: | ---: | ---: |
| 1,000 creations, updates and reads | 745.91 ms | 812.65 ms | 8.9% |
| 500 String ordering comparisons | 711.17 ms | 874.34 ms | 22.9% |

Owner-thread CPU ranges were 744.46–752.05 / 812.43–813.78 ms for Cells and
709.86–715.64 / 867.39–877.48 ms for String ordering. Every run had zero additional
facts and a 22-object post-GC process delta, including results and measurement
temporaries. Startup, preparation, result checking and explicit post-batch GC are
outside the timer; normal automatic GC remains enabled. These are two bounded
workloads, not a whole-language slowdown estimate.

The ownership and suspension requirements are implemented without new native Cell
policy. The cost is explicit rather than hidden behind synchronous compilation
or a special native Cell representation. Generic ordinary-slot access and small
authored accessor methods are the next performance investigation targets; no
specific optimization or recovery of this cost is claimed here.

Reproduction:

```sh
python3 scripts/cell-without-store-test.py PREFIX
BIRNAM_TEST_PREFIX=PREFIX bash scripts/birnam-test.sh
python3 scripts/local-cell-test.py PREFIX --legacy-prefix OLD_LOCAL_PREFIX --legacy-local
python3 scripts/local-cell-test.py PREFIX --legacy-prefix OLD_FACT_PREFIX
python3 scripts/local-cell-bench.py OLD_LOCAL_PREFIX PREFIX --baseline-local
python3 scripts/check-rooting-model.py
```

## Cost follow-up

The initial regression above is retained as the migration baseline. The subsequent
[slot and String cost investigation](slot-and-string-cost.md) removes private
root-token traffic from ObjectModel and mutable Cells from String ordering. Its
separate measurements distinguish the generic owner improvement from the library
algorithm change; the Cell API and ordinary-object representation remain intact.
