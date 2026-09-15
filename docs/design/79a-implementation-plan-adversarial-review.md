# 79A — Adversarial review of the implementation plan

> **Final plan review: findings addressed, 2026-09-15.** Independent review of
> [Design 79](79-refactoring-implementation-plan.md) against Designs
> [78](78-invariant-preserving-refactoring-review.md) and
> [78A](78a-refactoring-adversarial-review.md), with source inspected at
> `b8d9f255`. This is a planning review; no production implementation or
> runtime qualification occurred.

## Verdict

The proposed sequencing and scope do not require changing malleability or the
other governing invariants. The plan explicitly retains the difficult
boundaries: live dispatch authority, arbitrary native participation without
metadata, no replay, raw-pointer output publication, native retirement
authority, caller-specific recovery admission and independent semantic oracles.

The initial review found two medium-severity evidence gaps. Both could make a
superficially passing implementation test miss the changed behavior. The revised
plan addresses both; no material objection remains at the planning level. The
findings below describe the initial omissions and concern proposed evidence,
not confirmed production defects. Acceptance of this plan does not authorize
production implementation or establish semantic equivalence.

## Findings

### F1 — Medium: loader instrumentation can change the floor being tested

**Plan locations:** B1, the paragraph following B5, and section 5's new proof
obligations.

All four discovery implementations select an image by asking `dladdr` about
the address of `api.dlsym`:

- TCC `floor_symbol_address` and `floor_retirement`,
  [`rust/birnam-tcc/src/lib.rs`](../../rust/birnam-tcc/src/lib.rs), lines 241–318.
- Codegen `floor_symbol`,
  [`rust/birnam-codegen-runtime/src/lib.rs`](../../rust/birnam-codegen-runtime/src/lib.rs),
  lines 1806–1845.
- Image `floor_symbol`,
  [`rust/birnam-image/src/lib.rs`](../../rust/birnam-image/src/lib.rs), lines
  578–616.

B1 asks for real competing images and callback counts. Its instrumentation
paragraph tells the implementer to wrap the actual adapters' capability
callbacks, but does not preserve this address-identity condition.

**Counterexample:** a test replaces `api.dlsym` with a tracing wrapper defined
in its executable or helper library. Discovery now opens that wrapper's image,
not the intended floor. A forwarding wrapper or fallback lookup can still find
the expected symbol and produce passing callback counts. The test has stopped
proving exact selected-floor binding even though it invokes the real adapter.
An acquisition-failure assertion may likewise pass for the wrong reason.

**Required plan change:** specify an instrumentation strategy that preserves
image ownership. For example, each fixture floor can export its own tracing
`dlsym` trampoline, with that exact address in its IdApi and real handle-scoped
lookup underneath; alternatively retain the selected production floor's actual
`api.dlsym` pointer and use instrumentation that does not replace it. Verify the
owning image of that address and the acquired image in the receipt. Give
competing same-named exports distinct observable identities. Forbid fixture
global-lookup fallbacks, and distinguish deliberate callback failure from
failure to identify the correct image. Preserve fresh-process cache isolation.

### F2 — Medium: the default practical screen does not exercise F's changes

**Plan locations:** F0–F3, F series acceptance, and section 5's practical cost
screen route.

[`scripts/validate.py`](../../scripts/validate.py), lines 35–40, invokes
`performance-smoke.py` without a switch selection. The latter defaults to
`discarded-clones` and `virtual-fields` at lines 20–21. Its comparisons at lines
48–69 toggle those switches within one installed revision. They do not compare
the old and refactored implementation. Its reported metrics are execution time
and process wall time; the benchmark's `materialize-us` output is discarded
from observable-output comparison at line 60 rather than compared as a
preparation metric.

F1 and F2 target numeric closure-body and ordinary numeric/unboxed preparation.
F0's accepted/disabled/refused inventory is useful, but the acceptance text does
not explicitly require affected-family selection or baseline-versus-candidate
preparation measurements. The generic practical screen alone cannot establish
either.

**Counterexample:** analysis reuse accidentally doubles numeric closure-body
preparation work or extends an intermediate allocation's lifetime. Default
performance-screen processes leave that family disabled and pass. Even a
parameterized off/on comparison can conceal a regression shared by both
settings after the refactor; it is a different comparison from before/after.

**Required plan change:** retain the ordinary-default screen and add an explicit
affected-path recipe for each selected F change. Record numeric/closure switch
and scenario selection, and counters proving that the intended accepted
preparation path ran. Compare a matched reference revision/prefix and candidate
revision/prefix under the same build, inputs, feature flags and GC mode. Retain
preparation counts, its relevant cost, peak/live/retained storage and unchanged
admission/refusal outcomes; include disabled and refused cases to catch eager
work. Use disposable checkouts/prefixes or retained matching baseline evidence
without changing the working tree. This remains a bounded regression screen,
not a qualified speedup claim, default promotion or full performance study.

## Other challenges checked

These checks produced no additional material objection to the initial plan:

| Challenge | Source evidence and assessment |
| --- | --- |
| Hidden crate dependency | Pure Codegen already depends on Artifact with `default-features = false`; Artifact already depends on ABI. A2 can use the existing provider-free dependency path. Provider/export checks remain necessary. |
| Literal-envelope unification | C emission strips the first/last byte on any input of length at least two, while portable/runtime decoding validate quotes. A4 keeps admission outside the shared body decoder and A1 requires malformed-envelope cases. |
| FFI alias-test validity | Rule parsing copies descriptors into owned values before output writes. The exported pointer contracts and each actual overlapping allocation still need C1's validity audit; arbitrary overlapping typed references are not justified. C1/C2 explicitly require actual-adapter tests and completed callback/input work before writing. |
| Root and callback ownership | `mark_live_units` traverses separate registries, including a whole-callback early return if `MODULE_SOURCES.lock()` fails. D's same traversal/lock behavior requirement must include such short-circuit behavior when a segment becomes a helper; a helper-local return is not automatically equivalent. Root trace/source review can establish this without inventing a new locking policy. |
| Arbitrary native and live authority | The complete Design 78 ledger is mandatory, and D requires handwritten native/no-metadata, one-effect, reentry, next-send replacement and old-code retention evidence. No wrapper or recovery map is admitted as authority to replay opaque native effects. |
| Machine and StepAction | The incorporated ledger preserves microcode replacement and reflection, while action recognition remains canonical and directly decoded. The plan does not propose a new send for interpreting actions. |
| Host address lifetime | The current bridge stores a raw `HostRuntime` address and `Drop` clears by matching address. E4/E5 explicitly preserve representation, construction, installation and drop behavior; extraction does not authorize moving it into a new owner. |
| Recovery admission and independent oracles | G shares closure mechanics first; numeric/aggregate callers have independent gates. The public aggregate interface keeps noncanonical admission, and independent Frame/interpreter fixtures cannot call the new helper. |
| Tracker and implementation scope | Investigation gates do not become ready implementation tickets automatically. No execution, deployment, default promotion or tracker transition is authorized by this planning turn. |
| Test plan completeness | The plan correctly distinguishes workspace/native coverage from language/project fixtures and diagnostic evidence. New fixture sources must be hashed and wired into durable validation rather than credited from a manual log. |

## Final rereview and resolutions

I reread the revised B instructions, F0 revision-comparison protocol and the
validation entry-point explanation after the author addressed the findings.
I also checked the final additions: D expressly preserves whole-callback exits
and mutex failure behavior, including the `MODULE_SOURCES` short circuit; F
distinguishes peak/live transient storage, retained accounting and teardown,
treats peak RSS as supplementary, and defers reuse when required accounting
or lifetime evidence cannot be collected.

| Finding | Revised requirement | Disposition |
| --- | --- | --- |
| F1 | B requires image-owned fixture `dlsym` trampolines or address-preserving observation, asserts the owning image/path/handle and distinct returned symbol, prohibits global lookup, retains fresh-process isolation and prevents harness references from hiding early release. Normal-floor tests remain required. | Addressed. The instrumentation no longer grants itself a different selected-image authority. |
| F2 | F requires distinct baseline/candidate source snapshots and prefixes, matching harness/configuration with provenance checked against each snapshot, explicit affected-family switches and positive path markers, disabled/refused/limit cases, and preparation counts/time and lifetime evidence. Three alternating revision pairs use the stated practical regression rule; the generic aggregate off/on screen is supplementary. Missing targeted evidence defers the change. | Addressed. The acceptance comparison now exercises the changed path and compares the relevant revisions. |

Neither resolution changes dispatch, Machine/StepAction protocols, native
admission, root/retirement authority, FFI contracts or recovery-layout
admission. Neither weakens a semantic assertion or promotes an optimization
default. The plan is ready to guide selected implementation work when that
work is authorized, with its investigation gates still mandatory.

## Review limits

I inspected the proposed commits, relevant crate dependency files, byte/literal
callers, the actual loader adapters, relational parse/write boundaries,
Codegen root registries, host bridge ownership, optimizer preparation entry
points and validation/performance scripts. I did not execute the illustrative
counterexamples, run heavy test suites or prove all possible implementation
equivalences. Existing passing receipts cannot settle these proposed changes.

Each production series still requires its own characterization, source review
and acceptance evidence. A future implementation can violate this plan even
though the plan itself preserves the governing invariants; code and measured
behavior remain the acceptance evidence.
