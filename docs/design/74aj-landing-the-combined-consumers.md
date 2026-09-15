# 74AJ — Land the implemented four-family consumers

Status: accepted for integration, 2026-09-14. All 24 landing steps passed.
Existing defaults are unchanged; unboxed arithmetic, loop-invariant reuse,
closure elision, literal callees and counted Frame indices remain opt-in.
The remaining four-family qualification work remains open.

The user requested a shorter path to landing after the work expanded into
resource/performance research. Merge the tested 74AG candidate into current main
and reuse completed checks for unchanged inputs. All 314 production inputs match
74AG exactly; only the main-specific status document required conflict resolution.
The existing 672 compiler/runtime/module tests, 72 source lifetime/identity
processes and 168 lifecycle cycles remain applicable to this exact source.

The landing check builds and publishes a fresh normal release prefix. It checks
the installed export surface, image and portable-code lifecycles, stackless
control, native API context, ordinary recovery fixtures, changed numeric inputs,
numeric closure capture/suspension, String/finalizer interactions, and the ordinary
binding application. The source-only cold archive and relocated execution checks
remain included. It does not restart the exhaustive optimization matrices or add
timing experiments. The concrete runner is preserved in `74aj-landing-resumed.json`.

The initial export comparison used a bare Cargo floor as the installed-floor
baseline. Published floors also contain the existing module initializer and
symbol-borrow wrapper. The corrected comparison binds the existing published
floor's hash and exact 26-symbol surface; TCC and Codegen retain their exact
30/44-symbol comparisons. The successful release installation is reused without
rebuilding, and the initial failed comparison remains recorded.

The new 74AI Rust allocator experiment is preserved separately at `9c399703` and
is excluded from this integration. Its unfinished field/release checks are not
passes. Broader Rust/process resource accounting, further application/performance
research and any promotion of experimental defaults are follow-ups. Stopping
those experiments does not change the language contract or declare the full
four-family objective complete.

This landing acceptance supersedes 74AG's earlier requirement to finish the
exhaustive replay before merging. It does not claim that the interrupted full
production gate passed. The same frozen IdApi, Method ABI, descriptor/image
formats, mutation/identity rules, allocation/GC behavior and stackless ownership
contracts continue to apply.

## Landing result

[The terminal landing receipt](../measurements/74aj-landing-resumed.json) records
all 24 successful steps. The final audit rechecked all 314 production inputs,
256 installed-file hashes, step logs and child receipts. Both independent
fixture-free source archives built, installed and ran from relocated prefixes.
Changed-input counting controls passed 20 cases, numeric closure V2 controls
passed 40, and String/finalizer interactions passed 120. The ordinary application
passed all four consumer-off/on and GC-mode combinations on the fresh release.

This acceptance reuses the completed 672 regular tests, 72 source processes and
168 lifecycle cycles for identical source inputs. It does not claim a completed
exhaustive production replay or qualified speedup. The stopped replay and Rust
allocator experiment are deferred work, not prerequisites for this merge.

## Requirement audit after landing

[The requirement audit](../measurements/74aj-requirement-audit.json) maps the
original implementation categories to passing debug/release command results:

| Category | Verified behavior in the retained tests |
| --- | --- |
| Shared recovery | Id-authored recipes, graph validation, distinct identities and aliases, collecting materialization, stale authority refusal |
| Numeric representation | Exact arithmetic, original error boundaries, raw-result recovery, late binding, GC budgets and bounded storage |
| Loop invariants | Actual tail-iteration evidence, changed inputs and bindings, ordered effects, capture/suspension and native retirement |
| Aggregate allocation and fields | Clone/field plan admission, exact logical allocation observations, alias recovery, collecting observers and forced exits |
| Closures and captures | Current constructor/API authority, shared captures, ordinary refusal and staging; installed numeric and V2 cases in the landing receipt |
| Compiler ownership | Native allocation and compiler-heap lifetime, complete consumer teardown and source closure ownership |

The audit rechecks 314 current source hashes, eight compatibility-authority
files and the retained receipt bindings. It reuses the 74 individually successful
commands from the interrupted gate; it does not claim that gate completed. The
24 focused landing steps separately establish installed/image/archive acceptance.
No new semantic implementation gap was identified in these reviewed categories.
Full qualification remains unproven: the broader resource/profitability work and
experimental default promotion are deferred. This audit neither restarts those
experiments nor closes the persistent full-scope goal.
