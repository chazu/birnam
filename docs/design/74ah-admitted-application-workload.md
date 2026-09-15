# 74AH — Ordinary Id application acceptance with active consumers

Status: application source and driver integrated after 74AJ, 2026-09-14.
The combined runtime is on main; broader family qualification remains open.

Implementation, driver and measurement receipts are committed at `d53d56dd` on
`codex/74ah-admitted-application` in `/tmp/birnam-74ah-admitted-application`.
The application source and driver are now also on main, byte-identical to that
commit. Measurement links below refer to the original branch's diagnostic evidence.
74AJ integrated the runtime and verified four normal installed application cases;
the AH-only diagnostic counters remain on the original branch.

The existing collection/evaluation application controls admit no aggregate plans.
Keep those refusal/regression workloads. Add a binding-processing pipeline whose
ordinary Id source snapshots LogicVars, projects names, repeatedly transforms
values in a private arithmetic closure, and accumulates position-dependent
weights in a stackless loop. All computation and branching remains ordinary Id;
no diagnostic native method implements the application or grants eligibility.

The application must preserve full results and original records, react to changed
numeric inputs, and remain callable after cache eviction and collection. Compare
normal and diagnostic floors, individual consumer controls, combined controls,
GC modes and observing/mutating variants. Counter deltas must show actual numeric,
loop, field and closure execution before attributing timing differences. An
eligible-looking source body with zero measured activity is not accepted evidence
of benefit. Any runtime/compiler refusal discovered here needs a separately
reasoned implementation or explicit accounting; do not weaken the assertions.

Reuse the earlier isolated aggregate preparation/execution counters under
`ffi-test-hooks` and expose a read-only combined snapshot on the existing test
probe. Keep public ABI, normal-build behavior, Id semantics and all recovery guards
unchanged. Preserve source and artifact hashes for each actual executed floor.
The new fixture's initial copied-host runs are implementation checks, not fresh
installed production acceptance. Full candidate production/installed acceptance,
application mutation/identity checks, and profitability evidence remain needed.

## Application acceptance

The first application slice now passes 48 processes: debug/release, conservative/
precise GC, normal/diagnostic floors, and six configurations (all disabled, all
enabled, and each of numeric/loop/closure/field consumers disabled individually).
The normal floor and compiler modules are preserved 74AG artifacts; diagnostics
use the 74AH floor. This is explicitly recorded in the
[completion audit](../measurements/74ah-application-completion-audit.json).

Every configuration returns 45,152, responds to changed factor/bias with 47,584,
and survives materialization eviction and collection. After warmup, an ordinary
Id replacement of one record's getter changes the result to 63,152 and performs
exactly three observable calls. Those calls receive one shared fresh clone,
distinct from its original record; the retained clone survives collection.
Removing the replacement restores the original result.

Each fully enabled measured interval executes 1,024 unboxed numeric operations,
reuses 945 invariant predictions, completes 192 closure calls across 64 eliminated
closures, and virtualizes 64 field-bearing objects. No field object is reified
and no delayed allocation is settled during that interval. All four combinations
of profile/GC mode have identical activity. These counters prove execution and
allocation omission for this interval, not a speedup or absence of later logical
allocation settlement. Individual controls suppress their associated activity.

`scripts/recovery-binding-pipeline-smoke.py` is the durable source driver. It
checks exact outputs and activity, binds source/module/published-host hashes,
and rejects overwriting old receipts. The audit also preserves the eight normal/
diagnostic artifacts outside Cargo caches at `/tmp/birnam-74ah-proof-artifacts`.
The initial mutation fixture had an unmatched bracket and was rejected before
execution; that failed receipt is retained alongside the corrected acceptance.
Changed Rust files pass formatting checks. Workspace-wide formatting still
reports pre-existing differences in unchanged `dev_slot_access.rs`.

The consolidated workflow deliberately excludes additional noisy timing matrices.
74AJ completed focused production/fresh-install acceptance and superseded the
exhaustive replay for landing. Application resource and profitability research
are deferred. This application result does not promote any optimization default.

## Reproduce from main

With a current installed prefix, run `birnam bench/recovery-binding-pipeline.birnam`
for the ordinary correctness case. The driver below compares all-disabled,
all-enabled and individual consumer controls under both GC modes:

```sh
python3 scripts/recovery-binding-pipeline-smoke.py \
  --prefix /path/to/prefix \
  --floor /path/to/prefix/lib/libbirnam_id.dylib \
  --tcc /path/to/prefix/lib/libbirnam_id_tcc.dylib \
  --codegen /path/to/prefix/lib/libbirnam_id_codegen.dylib \
  --output /tmp/binding-pipeline-new.json
```

`--diagnostic` requires the AH diagnostic floor with `applicationActivity`;
main's ordinary floor does not expose that test probe. Normal mode needs no
diagnostic floor. The driver refuses to overwrite an existing receipt.

[The reproduction receipt](../measurements/74aj-application-reproducibility.json)
retains the four previously executed normal landing outputs and revalidates them
with the unchanged driver. It does not represent another test or timing matrix.
