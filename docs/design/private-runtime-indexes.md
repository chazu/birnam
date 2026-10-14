# Private runtime integer indexes

2026-10-13, based on `12f6aa70`. Follow-up to the Store/scratch investigation:
reduce general runtime overhead without moving public state or changing sends.
Measurement/acceptance evidence is in
[the receipt](../measurements/private-runtime-indexes-20261013.json).

## Kept change

Two private indexes now use the integer mixer already used by Id's address index,
instead of `RandomState`/SipHash:

- [Execution object records](../../rust/birnam-execution/src/objects.rs), keyed by
  the addresses of its own objects.
- [Root registry token positions](../../rust/birnam-root-registry/src/lib.rs),
  keyed by the registry's monotonically generated native Word tokens.

Both still use Rust's HashMap and full `usize` key equality. Only the bucket hash
changes. Separate small implementation modules keep this policy private to each
owner; the common Id ABI gains no utility, representation or service dependency.
The helpers are not language-level hashing APIs.

The hash mixes aligned addresses and sequential tokens into both bucket and
fingerprint bits. A raw identity hash was not chosen: sequential/aligned inputs
would use the hash table's fingerprint bits poorly. A vector indexed by token
was not chosen either: tokens can become arbitrarily large while few leases stay
live, and old outstanding tokens cannot be reused or renumbered. Neither
alternative was implemented or benchmarked in this change.

Byte/string hashing, user Map equality/hashing, selector resolution, method
lookup and all other runtime tables are untouched. In particular, this does not
turn native bookkeeping into a replacement for user-defined equality.

## Invariants and ownership

Execution's table is a weak record index, not an allocation or root authority.
Id still allocates and retires the payload. Record insertion, family assignment,
behavior refresh, removal, payload extent checks and re-registration at a reused
address follow their existing paths. The table has no observable enumeration.

Roots still owns its provider Vector and `(token, object)` Vector. These remain
the enumeration authority, including duplicate references and existing swap-removal
order. The positions index only locates a token's current Vector slot. Null
rejection, unknown/double release, monotonic token identity and exhaustion behavior
remain the same. The change introduces neither dense token allocation nor lease
reuse.

These are private representation changes, not speculative dispatch shortcuts.
No owner message is bypassed, no borrowed state is held over a new callback, and
no method/context is frozen. Existing lookup, replacement, guarded admission,
recovery and code-lifetime protocols remain in force. No new deoptimization or
monkeypatching check is introduced.

## Profiling and measurement

The baseline is the compiled prefix containing the committed scratch fixes,
`/tmp/birnam-scratch-state/prefix`, rather than the older installed 0.17.0
implementation without those fixes. The candidate is rebuilt separately at
`/tmp/birnam-runtime-tables/candidate`.

A fresh five-second warmed Logue sample found Execution hashing and Root index
insertion among the largest named leaf costs. Sampled runs verified their final
256-turn state against the independent game oracle. Idle stdin samples were
excluded from active-sample counts. Dynamic generated code is not fully symbolized,
so samples locate costs rather than provide complete source-level attribution.

Application timing used existing oracle-backed runners, with no application edits.
Three fresh process runs per variant/workload alternated baseline/candidate order.
Timings were sequential and separate from builds, profilers and qualification.
Normal automatic GC remained enabled.

| Workload | Baseline median CPU, ms | Candidate median CPU, ms | Reduction |
| --- | ---: | ---: | ---: |
| Logue: 16 crowded turns | 726.28 | 695.00 | 4.3% |
| Logue: 16 quiet turns | 21.855 | 21.038 | 3.7% |
| Logue: level generation | 2,165.15 | 2,016.56 | 6.9% |
| JSON: complete 50-record report | 268.49 | 257.15 | 4.2% |

These are medians of each process's batch/phase median. Logue timing excludes
loading, arena preparation, snapshots and between-batch explicit GC. JSON timing
uses five independently warmed phase repetitions with prepared input graphs;
phases are not additive attribution. Initialization/process elapsed time is not
the quantity in this table.

Baseline/candidate process-median ranges respectively:

- Crowded turns: 710.57–727.80 / 680.39–698.13 ms.
- Quiet turns: 21.552–22.363 / 20.856–21.084 ms.
- Generation: 2,128.91–2,169.28 / 2,016.45–2,035.59 ms.
- Full JSON report: 268.48–269.13 / 256.59–258.28 ms.

The ranges are separated in these runs; they are not a universal performance
bound. The measured benefit belongs to the two-index change together, not an
independently measured percentage for each index.

The JSON allocation diagnostic still reports 91,404 objects and 2,548,666 charged
bytes for a complete report on both variants. That diagnostic disables automatic
GC for one separate pass; its CPU time is not the timing baseline. This is reduced
bookkeeping cost rather than language-object allocation elimination.

## Verification

Owner tests add two kinds of witnesses:

- Execution records are compared with an independent BTreeMap across thousands
  of aligned/high-bit/extreme addresses, removal and address reuse, including
  family/behavior changes. Real-buffer tests check membership and payload bounds
  before insertion, after removal and with a shorter reused extent.
- Root operations are compared with a simple reference Vector under mixed
  retain/release/null/unknown-token operations. Tests compare the exact Vector
  order and every token's position, alongside duplicates and near-`usize::MAX`
  token exhaustion. Existing reentrant/batch tests remain active.

[PrivateIndex.lean](../../formal/semantics/lean/PrivateIndex.lean) models an arbitrary
hash, including a constant hash. It proves collision-safe lookup, equivalence
under changing hashes, distinct colliding keys, rejection of an unknown token,
and unchanged authoritative root order/duplicates. It abstracts the standard
HashMap contract; it does not verify Rust's library, the mixer's distribution,
or unsafe payload access. Negative mutations deliberately confuse collisions
with identity and reverse the root sequence; both must be rejected.

Completed acceptance:

- 31 focused owner unit tests passed.
- All 66 native integration binaries passed (68 top-level tests, plus isolated
  child cases). This includes GC, roots, execution, compiler, replacement,
  compensation, code reclamation and image roundtrip/resave checks.
- 718 language checks and the private-scratch regression gate passed.
- 218 Lean theorems passed kernel axiom auditing; 47 invalid mutations were
  rejected, including the new collision and ordering cases.
- All 123 existing TLC configurations produced their required passing results or
  expected counterexamples. No new temporal dispatch protocol is introduced here.

Exact logs/counts and source/runtime bindings are recorded in the receipt. The
native validation prefix enables TCC resource metrics to check real code
reclamation; that instrumented prefix is separate from performance measurement.
The native boundary audit checks exports, install names and dependencies.

## Reproduction

Build a separate prefix using `scripts/rust-install.sh`; a Cargo cache may be
reused, but authored bootstrap/tools and images are regenerated from source.
Keep the baseline prefix immutable. Existing workloads:

```sh
python3 scripts/logue-bench.py --prefix PREFIX --work WORK \
  --cases crowded,quiet,generation --samples 1
python3 scripts/log-summary-profile.py --prefix PREFIX --work WORK --iterations 5
mise exec -- cargo test --locked --offline -p birnam-execution -p birnam-root-registry
BIRNAM_TEST_PREFIX=PREFIX bash scripts/birnam-test.sh
python3 scripts/check-rooting-model.py
```

Use distinct work directories and alternate prefixes for repeated measurements.
The receipt records commands, source/runtime hashes, individual samples, oracle
results and the separate profile summaries. No application-specific shortcut,
GC threshold change or permanent foreign service table is part of this work.
