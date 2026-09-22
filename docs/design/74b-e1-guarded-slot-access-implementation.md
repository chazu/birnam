# 74B — E1: guarded Machine slot access, development-only artifact

> **Status: delivered 2026-09-09 for review, under the user's E1 authorization
> of the same day (Design 74 §7.3). Development-only: nothing here changes an
> ordinary build, an admitted artifact, the frozen IdApi, or the runtime ABI
> hash. E2's remaining matrix rows and the formal E3 measurement with its
> frozen 20% criterion are not authorized by this document.**

## 1. What was built

The [Design 74A](74a-e0-guarded-slot-access-legality.md) §5 sketch, exactly:

| Piece | Where | Behavior |
| --- | --- | --- |
| Dictionary-mutation epoch | `Vtables::mutation_epoch` / `note_mutation` in [`vtable.rs`](../../rust/birnam-id/src/id/vtable.rs) | Bumped by `add_method` (both outcomes), `remove_method`, `add_weak_opaque_binding`, `remove_dead_weak_bindings` and `remove_dead_collectable` (when they removed anything), `set_parent`, and `Runtime::ensure_private_vtable`. Saturating; never reset. |
| Eligibility | `Runtime::guarded_slot_access_eligible` in [`runtime.rs`](../../rust/birnam-runtime/src/runtime.rs) | Conditions 2–4 (canonical Machine vtable's direct `oopAt:`/`oopAt:put:` entries are the floor Methods; its metavtable is `vtable_vt`; `vtable_vt` is its own metavtable and its direct `lookup` is the floor primitive) are re-verified only when the epoch moved and cached with it. Condition 1 (the receiver's checked header vtable is the canonical Machine vtable) is read on every call. |
| Direct access | `Runtime::guarded_frame_oop_at` / `_put` | `None` when ineligible; otherwise the canonical Method's body minus dispatch: same index decoding, same `stackless_oop_at` / `_put`, every `Err` mapped to nil. |
| Floor exports | [`dev_slot_access.rs`](../../rust/birnam-runtime/src/dev_slot_access.rs), feature `dev-guarded-slots` only | `birnam_dev_frame_oop_at_v1(machine, box, out)` and `_put_v1(machine, box, value, out)` answer 1 on a hit (writing `out`) and 0 when ineligible. Optional counters (`BIRNAM_DEV_GUARDED_SLOTS_COUNT=1`) print attempts/hits/misses at process exit. |
| TCC rung shims | [`birnam-tcc/src/lib.rs`](../../rust/birnam-tcc/src/lib.rs), feature only | Same two names, forwarded to the selected floor through the existing private-symbol discovery (`floor_symbol_address`, extracted from the registration forwarders); present in the generated-code symbol table and the validation table under the feature. |
| Emission | [`resumable_c.rs`](../../rust/birnam-codegen/src/resumable_c.rs) `guarded_slots_selected`, feature only | With `BIRNAM_DEV_GUARDED_SLOTS=1` in the compiling process, `emit_get` emits `__birnam_dev_get(machine, slot)` and `emit_store_suffix` emits `__birnam_dev_put(machine, selector, box, value)`; both helpers are appended to the runtime prelude. The slot box and the `oopAt:put:` selector are still produced before the value, as the baseline does; the miss path is the baseline send with that same box, once. |
| Build selection | `BIRNAM_BUILD_CARGO_FEATURES` in [`birnam-build.rs`](../../rust/birnam-build/src/main.rs) | Passed to cargo as `--features` and recorded in the install receipt's `cargo_invocation`, so a dev prefix is never mistaken for an ordinary one. |

A dev prefix is built with
`BIRNAM_BUILD_CARGO_FEATURES=birnam-id/dev-guarded-slots,birnam-tcc/dev-guarded-slots,birnam-codegen/dev-guarded-slots`;
it is never installed or published. Its stage1 artifacts use the baseline
emission (the environment variable is not set at build time). Only a unit
compiled inside a process that sets `BIRNAM_DEV_GUARDED_SLOTS=1` gets the
guarded form, and such a unit references symbols an ordinary runtime does not
export, so it cannot load there.

## 2. Why this is contained

- The runtime ABI hash folds `RUNTIME_SHIM_NAMES` and the registration ABI;
  neither changed, so every admitted artifact stays admitted.
- The ordinary floor and TCC dylibs export exactly what they exported before;
  the TCC export audit passes on an ordinary build of this tree (§4).
- `Vtables::mutation_epoch` and the eligibility cache exist in every build but
  are only read by the feature-gated paths; the epoch bump is one saturating
  add per dictionary mutation.
- No method address is cached in generated code; the hit path calls the same
  Runtime functions the canonical Methods call.

## 3. Fixture and diagnostic timing

[`bench/e1-slot-access.birnam`](../../bench/e1-slot-access.birnam) compiles a
unit in-process with whole-unit resumable selection (after recording the
module's baseline source and sealing the current generation, as the
compiled-unit suite does), then runs its own `e1To:do:answer:` control, a copy
of the production `to:do:` shape, with an empty body and a `step:` body that
accumulates into a captured Cell. Iteration count and unit path come from the
environment. Design 66's 100,000-iteration probe is unchanged.
[`scripts/e1-slot-access.py`](../../scripts/e1-slot-access.py) runs baseline
and candidate as fresh-process pairs in alternating order, checks the receiver
answer (7) and the checksum (7·n + n(n+1)/2) on every run, and reports medians
plus one separately instrumented candidate run's counters.

First diagnostic, five pairs per size, one dev prefix (`f24ff9b2` tree plus
this change), release profile, same host, default GC, all runs after each
other on a busy developer machine:

| n | baseline empty µs | guarded empty µs | Δ | baseline step µs | guarded step µs | Δ | attempts / hits |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| 0 | 51 | 45 | −11.8% | 50 | 40 | −20.0% | 122 / 122 |
| 1 | 81 | 76 | −6.2% | 82 | 73 | −11.0% | 371 / 371 |
| 32 | 898 | 694 | −22.7% | 1,091 | 840 | −23.0% | 8,090 / 8,090 |
| 1,024 | 23,654 | 19,149 | −19.0% | 32,410 | 25,182 | −22.3% | 255,098 / 255,098 |
| 16,384 | 547,822 | 464,154 | −15.3% | 660,980 | 559,911 | −15.3% | 4,079,738 / 4,079,738 |

Every guarded access hit; checksums and receiver answers matched the baseline
at every size. Per iteration the empty loop costs about 33 µs baseline and
28 µs guarded at 16,384. This is not the E3 measurement: it uses one prefix
in two modes rather than independent baseline/candidate prefixes, has no
Objective-C reference, does not separate startup/compilation/warm phases
beyond the ready marker, and was taken while other work ran on the host. The
attempt counts also include the StepMethods executed while the fixture's unit
was activated, not only the timed loops.

## 4. Correctness evidence

- `vtable::tests::every_dictionary_mutator_moves_the_mutation_epoch`: each
  mutator moves the epoch; sweeps that remove nothing do not.
- `runtime::tests::guarded_slot_access_agrees_with_the_canonical_send_and_misses_on_overrides`
  (feature-gated), inside a driven Machine: reads agree with the send for
  live Oop slots; raw slot, out-of-range index and non-Integer operand answer
  nil on both paths; guarded and sent writes are mutually visible and the raw
  slot write is rejected identically; redefining or removing either canonical
  entry misses and restoring it hits again; a custom `lookup` on `vtable_vt`
  misses and the floor primitive restored hits; a non-canonical metavtable on
  the Machine vtable misses and the canonical one restored hits; a private
  vtable on the Machine misses; the ordinary send keeps working after every
  miss; outside a drive the guarded read fails to nil exactly as the send.
- Module suites, workspace tests, audits and boot timing on an ordinary build
  of this tree, and the module suites on the dev prefix with the guarded
  emission selected for every unit they compile: see the validation record
  below.

### Validation record

All on 2026-09-09, same tree (`37f6e83c` plus this change):

| Check | Result |
| --- | --- |
| Ordinary release prefix: TCC export audit | passed; zero `dev_frame` symbols in the floor and TCC dylibs |
| Ordinary prefix: module suites | 627 passed, 0 failed |
| Dev prefix, `BIRNAM_DEV_GUARDED_SLOTS=1` for every unit the suites compile | 627 passed, 0 failed |
| Dev prefix, baseline emission | 627 passed, 0 failed |
| `cargo test --workspace` with the feature | every crate ok; floor 122 passed (the two new tests included) |
| `scripts/rust-cutover-verify.sh` (ordinary features) | workspace, native-prefix, cold-archive and relocated-runtime gates passed |
| Stdlib boot, 15 interleaved reps | v0.15.0-tree prefix 90.8 ms, ordinary prefix of this tree 91.3 ms, dev prefix 90.2 ms (the epoch bump is not measurable) |


## 5. Findings for 74A

`ensurePrivateVtable` on a vtable whose metavtable is already `vtable_vt`
answers the vtable itself (Runtime::ensure_private_vtable), so 74A §2.3 and
§2.5's "private vtable on the Machine vtable / on `vtable_vt`" cannot arise
through that path. A non-canonical metavtable can only be installed by
bootstrap or image code; conditions 3–4 still cover it and cost nothing
between epoch moves. 74A is amended to say so.

## 6. What remains for E2 and E3

E2 rows not yet exercised on the E1 artifact: replacing `step:` during
execution including during argument evaluation (source sends still cross the
driver, so the guard is not involved, but the matrix wants it recorded);
suspend/resume through the fully resumable host-drive seam; forced GC and
nested callback entry between guarded accesses; deliberate barrier and
foreign-owner attempts; repeated invalidation and abandoned execution bounds.
The unit test covers the override, lookup, metavtable, private-vtable,
invalid-input and miss-then-send rows.

E3 needs independent baseline and candidate prefixes from fixed source states,
at least five alternating fresh-process pairs with all samples reported,
separated startup/compilation/warm phases, the default-GC and explicit-GC
variants, generated-code inspection, the two Objective-C references, and the
20% criterion frozen before measurement. On this first diagnostic the
candidate clears 20% at 32 and 1,024 iterations on the step body and sits at
15% at 16,384, so the criterion is not yet met or missed with the required
evidence.
