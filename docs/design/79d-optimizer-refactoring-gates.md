# 79D — Optimizer refactoring gate decisions

Status: F deferred; G1/G2 implemented, integrated acceptance pending. This record
applies to the Design 79 batch through `2e8d14d6`. It does not promote any native
family or change the live guards that authorize optimized execution.

## F: analysis reuse remains deferred

The source audit confirms repeated pure analysis. It does not yet establish a
qualified cost improvement or an unchanged preparation-lifetime envelope.

| Production preparation path | Existing work and boundary | Constraint on reuse |
| --- | --- | --- |
| `native_tier::Prepared::new` | Lowers and colors once, builds bounded native source, derives a boxed numeric Template, then separately asks `unboxed_numeric::prepare_maps` for maps. | Native source is limited to 256 KiB. The maps entry returns before derivation when the unboxed family is disabled. Reuse must not eagerly prepare maps. |
| `unboxed_numeric::prepare_maps` | With the family enabled, `RecoveryMaps::derive` derives its own Template and requires exact canonical coloring. Preparation attempts, successes and elapsed time are separately recorded. | The earlier boxed Template permits layouts that RecoveryMaps rejects. Sharing must retain the stronger map check without narrowing the boxed caller. |
| `closure_elision::NumericBody::derive` | Derives Template, then RecoveryMaps, which derives Template again. Retains the first Template, maps, allocation sequence and publication rows. | This is the most bounded candidate. Eliminating the second Template still needs the prescribed accepted/refused preparation counts and transient/retained ownership comparison. Equal plans alone do not supply that evidence. |
| `closure_elision::Prepared::new` | Lowers; refuses literal-free first blocks before coloring; tries leaf derivation before numeric fallback. Both derivations independently validate the outer layout and lower/color the nested body. | A shared owner must preserve refusal order and leaf-first laziness. It cannot retain compiler inputs across the runtime's Pending/Ready/Refused boundary. |
| Closure pending inputs | Owns source/selector/expected-descriptor bytes with 256 KiB per-input and 4 MiB aggregate reservations; reservation Drop releases the charge. | A retained analysis object is not represented by those byte reservations. Do not attach one to pending records or treat input-byte accounting as its cost. |

For a successful numeric-body path, the source call graph contains two Template
derivations and one additional canonical-color check. An enabled ordinary
numeric/maps preparation likewise repeats Template derivation. These are static
call-path counts, **not measured production counters or timing results**. Early
and late refusal paths need separate measurement; disabled maps do no map work.

No F1–F3 production edits are included. The required paired, source-bound normal
release comparison has not been produced: targeted successful/refused/disabled
markers, three alternating pairs, analysis counts and transient/retained lifetime
accounting remain open. Therefore F stays deferred at its evidence gate. This is
not a claim that reuse is unprofitable, and the F investigation ticket remains
open. The existing off/on performance harness cannot stand in for that comparison.

## G: closure-only transition sharing

| Caller | Admitted domain and old transition | Decision |
| --- | --- | --- |
| First closure `Plan::derive_with` | Requires exact `color_slots(program)` equality after its existing size/shape gates; at most 128 slots. Copies read old aliases, result becomes false, then clear bits become false. Missing result maps to `InvalidLayout`. | Delegate only this pure transition; retain all caller checks and refusal adaptation. |
| Closure `sequence::extend` | Private continuation of an already-validated first plan. Copies read old aliases, result becomes false, then per-slot bitmap reads clear aliases. A missing bitmap word means no clear in the loop, although a truncated canonical bitmap cannot reach it through the public caller. Missing result returns None. | Use the same private helper. Keep Option propagation and sequence exit/size policy. |
| Numeric `RecoveryMaps::derive` | Independently validates Template and exact canonical layout. Copies old result identities, clears aliases, then writes the result slot to None. | Unchanged. Result/clear ordering is distinct; the bool closure helper is not a generic result-identity API. |
| Aggregate `Plan::derive` | Public bounded derivation accepts some noncanonical layouts; individual reads/writes are checked. Copies are snapshotted, result receives Some(pc), then clears can erase it. Missing words perform no clears; an out-of-range set bit returns None. | Unchanged. Sharing must not impose canonical-layout equality or change result/clear behavior. |

Canonical `color_slots` currently constructs empty copy lists, complete bitmaps
with zero padding, and excludes the result slot from clears. Thus canonical
production fixtures cannot exercise a copy cycle or result/clear overlap. The
new helper's explicit literal expected-value tests cover those mechanics
separately, including the 63/64 bitmap boundary and missing-result return.
They do not claim those cases currently reach canonical closure preparation.

The pre-extraction caller characterization rejects seven edge corruptions at
each edge of a repeated-closure fixture: absent/out-of-range result, missing or
extra bitmap words, out-of-range set bits, and invalid copy source/destination.
The existing independent physical Frame comparisons exercise first and repeated
closure calls, captures, escaping aliases, call limits, numeric bodies and
allocation indices. None calls the new helper as an oracle. Both helper callers
still allocate one alias vector at the same preparation point; no Oop, runtime
root, native metadata, live send guard or execution allocation charge moved.

G0 baseline: 13 closure tests passed. G1 and G2 each passed all 97 pure Codegen
tests. These are focused results, not full series acceptance. The integrated
release matrix and language suite remain required. G3 is deferred because the
numeric and aggregate contracts above do not justify one shared transition.
