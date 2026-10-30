# Public Integer arithmetic performance investigation

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

The remaining Integer slowdown is primarily execution plumbing around arithmetic.
The guarded compiler passes remove allocations, but leave most messages, branch
Block construction, validation and root management in place. Further scratch-
buffer tuning alone will not close the gap.

## Scope and method

Investigated the current Birnam `+` policy on positive Integer operands. A
compiled synchronous fixture performs sixteen dependent public additions. The
baseline and candidate select source Integer methods compiled with the recent
three passes disabled/enabled; all runtime sends still use Id. Method selection
changes and warmup occur outside timing. Runs use a single worker at reduced
priority, two-second measurement/profile budgets and fixed batch sizes.

The diagnostic prefixes are copied installations with updated release libraries,
not published installations. No user installation or production arithmetic policy
was changed in this investigation. It does not measure image startup, every
operator, errors or mixed Float performance.

Two initial profiles exposed another harness issue: profiling sampled explicit
between-batch collection even with workload GC disabled. Continuous profiling now
initializes/collects once and runs the selected workload without those setup
collections or repeated method switches. Automatic normal-GC collection remains
included. Earlier batch-mode profiles are diagnostic only.

An optional `dispatch-metrics` feature records selectors inside Id's message
entry. It does not replace lookup, allocation or dispatch methods, and the default
build contains neither the counter fields nor recording path. `--counts-only`
uses a separate Id build and copied prefix and records no performance timings.
Counters begin after warmup/collection and stop before result validation. The
message-count endpoint itself is excluded from totals. The external fixture call
adds one message to the sixteen-operation census. Counts include nested messages
issued by native and authored owners through the protocol; these are not all
source-level message expressions.

## Measurements

| Path | Candidate CPU cost | Physical work |
| --- | ---: | --- |
| Complete public addition | 283.4 us/addition | 54 objects, including eight Blocks and thirteen Words; 2000 bytes charge |
| Overflow/check/result core on already unboxed Word operands | 62.0 us/invocation | two Blocks, three Words, one Integer, eight other objects; 400 bytes charge |
| Retained native Integer reference | 2.18 us/addition | one Integer result; 16 bytes charge |

The native reference uses the older simple-send harness with per-result checks;
the public fixture is a compiled chain with normal GC. These numbers show scale,
not an exact matched native/Birnam slowdown ratio. The core fixture derives the
arithmetic body from the real module in a private factory and bypasses operand
validation/unboxing/operation selection intentionally. Its roughly 22% share of
full-path cost is a diagnostic estimate; the wrappers and inputs differ.

The public optimized/ordinary source ratio was 0.961, with an indicative within-
run interval of 0.943–0.972. This crosses the tooling's 3% benefit margin and is
reported as inconclusive. The core ratio was 0.952 (0.941–0.958), a small gain.

## Message explosion

One ordinary source addition triggers about 4404 protocol messages; the optimized
source path triggers about 4232. That is only a 3.9% reduction despite removal of
six Vectors, three Words and their associated backing allocations.

| Optimized path: messages per public addition | Count |
| --- | ---: |
| `retain:` | 738.5 |
| `release:` | 738.5 |
| `identityOf:` | 273 |
| `intern:` | 533 |
| `symbolNamed:` | 533 |
| `symbolBytesOf:` | 164 |
| Namespace `at:` | 168 |
| `prepareAllocation` | 124 |
| `captureWord:of:` | 69 |
| Block construction | 8 |
| Canonical CodeObject byte requests | 8 |

Root retain/release/identity traffic is about 41% of messages. Symbol requests
and name-byte inspection are about 29%. These are counts, not CPU percentages.
Protocol `send_named` repeatedly asks Id to intern spellings; the generated C
selector cache fixes literal-address duplication but does not eliminate native
owners' repeated named-send work.

The optimized core still issues 922 protocol messages per invocation, compared
with roughly 4232 for public addition. Its unchanged two branch Blocks account
for part of even that smaller path.

## Root overlap census

An untimed follow-up traces successful token-returning `retain:` and `release:`
messages through Id, recording the receiver/token domain and parent message
activation. It never changes the Root registry or removes a registration.

Across sixteen optimized public additions, the trace recorded 11,816 retains
and balanced releases, with zero observed tokens remaining:

| Registration at that instant | Per addition | Share |
| --- | ---: | ---: |
| Same object already protected in the same message activation | 192.0 | 26.0% |
| Same object already protected only by another activation | 328.2 | 44.4% |
| No overlapping token observed | 218.3 | 29.6% |
| Total | 738.5 | 100% |

Thus 520.2 registrations per addition (70.4%) overlap existing protection. This
is a measured optimization opportunity, not a proven removable count. The
existing token must outlive every use covered by the omitted token; message
activation boundaries are not necessarily generated `Call` scope boundaries.
Pre-existing persistent roots and transitive object reachability are outside
this census. The 790 distinct object addresses across the complete chain are
also not a minimum root count: an object may need separate protection during
non-overlapping lifetimes. Maximum simultaneous observed tokens was 262.

Start with duplicate protection within a generated `Call` scope: retain an
object once, preserve that token until scope exit, and reuse it for repeated
`keep` requests. Guard this against replaceable Root behavior and observable
root-enumeration semantics; do not simply delete messages under arbitrary
replacement. Then investigate borrowing ancestor protection where lifetime and
callback/reentry rules establish safety. All remaining registrations must still
use owner messages through Id. No production root elimination is implemented
by this measurement, and no speedup is claimed from these untimed counts.

The conditional coalescing and recovery contract is now modeled and checked in
[`formal/rooting`](../../../formal/rooting/README.md). Its Lean proof establishes
root/collector equivalence, not eligibility of today's production Root API.

Evidence: `docs/measurements/integer-root-overlap-20261006.json`. A focused
counter regression covers same-activation duplication, nested-call overlap,
release followed by a fresh retain, balanced cleanup, and census reset.

## Profile and source findings

The two-second continuous profiles account for 1676 samples with GC disabled and
1704 with normal GC. Inclusive categories overlap and must not be added together.

| Inclusive path | GC disabled | Normal GC |
| --- | ---: | ---: |
| Block construction (`new_block_input`, including dependencies) | 26.6% | 29.8% |
| Root registry operations and dependencies | 9.3% | 7.2% |
| ABI-literal adaptation path and dependencies | 6.5% | 5.2% |
| Guarded Word evaluator and dependencies | 1.2% | 1.1% |
| Collector run and dependencies | 0.1% | 25.8% |

Id dispatch/lookup/symbol functions themselves account for about 25–30% of leaf
samples, with additional hashing/copying/allocator time in generic or system
functions. Generated TCC frames are partly unnamed. Thus the report does not
claim an exact exhaustive CPU breakdown for each Birnam source helper.

Source inspection explains the measurements:

- Every branch literal allocates reference-slot capture storage, a delegated
  behavior and a real Block, even when that branch is not selected. The common
  valid-Integer path constructs eight Blocks and sixteen behavior objects.
- Each Block constructor asks CodeObject for canonical bytes, copies/validates
  its portable program to recover parameter/capture counts, enumerates captured
  slots, delegates behavior and installs the `value:` entry. These repeated
  eight-per-addition metadata requests occur for already admitted immutable code.
- Generated synchronous code retains receivers/arguments and Object results of
  sends, and releases those tokens at each function exit. Nested helper/adapter/
  Block invocations amplify these roots and their identity lookups.
- The common path validates the left operand, checks Error identity, validates
  the right operand, selects an operation from a Map, invokes its reusable Block,
  calls the bits helper and bridges the final result back to Integer.
- Guards add about 129 protocol messages per addition (roughly 3% of the total
  by the selected guard-query names). They can still be optimized, but the Word
  evaluator's 1% inclusive profile share makes its scratch buffers a small target.

## Recommended next fixes

1. **Make Block construction reuse admitted code shape.** Expose immutable
   parameter/capture counts through the CodeObject owner and reuse them without
   repeatedly copying and validating the complete portable program. Keep Block
   identity, capture checks, real allocation and current owner messages intact.
   This is simpler than eliminating observable Frames or rewriting Integer.
2. **Reduce root and repeated name-resolution work.** Profile/count generated
   caller roots and native named-send symbol traffic separately. Use existing
   owner responsibilities for batched/scoped roots and retained Symbols. Preserve
   GC liveness, callbacks, method replacement and code lifetime; do not use a
   compiler-side foreign payload or executable cache to avoid Id.
3. **Then handle branch construction with recovery.** It remains a large cost,
   but omission must preserve observable resumable Frames and materialize state
   when needed. Do not replace `ifTrue:ifFalse:` with native signed arithmetic or
   silently skip replaceable Boolean/Block behavior.

A narrower source experiment could remove Map/Block operation selection through
per-operator authored wrappers, while preserving left-validation/Error/Float
precedence and live helper replacement. That addresses one part of the plumbing,
not the full measured cost. It should be tested after the above owner metadata
and root/name work, rather than presumed to close the remaining gap.

## Evidence and limits

Raw JSON, samples and logs: `/tmp/birnam-integer-investigation/`. The permanent
summary is `docs/measurements/integer-public-path-20261006.json`. Counter endpoint
start/stop/reset preserves behavior epoch in its regression. Both default and
metrics-enabled builds compile; reporter tests pass. No further production
optimization was implemented, and no exhaustive runtime suite was repeated.
