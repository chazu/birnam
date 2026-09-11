# 74F — Same-frame deoptimization without an ABI cut

Status: experiment authorized and executed 2026-09-10; recovery checks passed
in debug and release. Protocol below was recorded before running.
The user selected the second path after the 74E reversal: preserve the current
runtime ABI, image format, and Frame representation while proving recovery.
Tracker: `birnam-deopt-same-frame-738`.

## Problem and scope

74A–74D prove fallback before an individual operation. They do not demonstrate
switching an active optimized computation back to its original implementation.
74E's new generated-code exports are not needed for a runtime-owned experiment.

This experiment uses the real Runtime driver, registered native StepMethods,
and descriptor-v2 Frames. It runs in the Rust test harness: a test-only private
tier record selects between two statically linked native entries. There is no
new export, compiler emission, artifact tier, saved metadata, or installed mode.
Hand-authored entries deliberately isolate recovery from compiler construction.
This is a recovery experiment, not a speed or general-optimizer qualification.

## Mechanism and equivalence

One runtime-local record retains the exact baseline descriptor by `Arc`, one
static optimized entry, an epoch, and a bounded list of eligible PCs. Selection
occurs after allocation of the existing resume token and before native entry.
It checks the exact descriptor allocation, PC, current dictionary epoch, and
74A's canonical Machine/lookup guard. An exhausted epoch never authorizes the
optimized entry. A failed dependency check permanently disables this trial;
the driver invokes the descriptor's original StepMethod with the same token.
No state borrow crosses entry or an ordinary send.

Both entries use the same descriptor, PC meanings, live-slot bitmap and ordinary
action publication. The optimized entry removes a second identical slot read
within an allocation-free, callback-free region whose Machine lookup is guarded.
The baseline reuses the already-boxed index for both reads. The optimized entry
does not carry assumptions across the later effect callback or any allocation.
Subsequent writes and action publication use ordinary sends.

PC 0 computes a value, stores it and an alias in canonical rooted slots, then
performs one effect callback. That callback may change lookup or replace the
currently active method. Both versions publish the same Continue edge to PC 1.
At PC 1 the driver can select the original baseline entry without copying or
reconstructing a Frame. The identity mapping is the recovery map: the next PC,
live Oops, aliases, return destination and active implementation stay in place.
Replacing a method binding must affect the next invocation, not replace the
already-active baseline body. Existing registration pins protect that body.

The tier owns no Oop roots or dynamically allocated executable code. Its only
candidate is a static test function. Retaining an `Arc<FrameDescriptor>` would
not by itself protect a dynamically unloadable code generation; admission and
leases for such candidates are explicitly outside this experiment.

## Frozen acceptance checks

- Run baseline and candidate independently; compare answers and effect traces.
- Prove optimized entry actually executes and removes the redundant read.
- Force invalidation after the effect; observe optimized PC 0 then baseline
  PC 1 on the same generational Frame handle and descriptor.
- Check no repeated effect or argument evaluation; preserve carried values and
  alias identity across recovery.
- Cover canonical execution, pre-entry invalidation, mutation then restoration,
  custom lookup, Machine override, and active method replacement. A later send
  must see the replacement while the active call finishes its original body.
- Force precise collection while values are retained only through the Frame;
  check a retained FrameView and its invalidation after return.
- Exercise nested registered entry and repeated trials; all Frames and pins
  must be released. Candidate selection must not attach to a different
  descriptor merely because its numeric identity matches.
- Leave runtime ABI inputs, IdApi, image/descriptor codecs and fixtures byte
  unchanged. Ordinary builds must contain no experiment code or exports.

## Running and results

Run from this checkout:

```sh
mise run deopt-experiment
# To retain the complete receipt:
python3 scripts/same-frame-deopt.py --output /tmp/74f-same-frame-deopt.json
```

The [driver](../../scripts/same-frame-deopt.py) runs both Rust profiles, requires
every matrix row and identical debug/release observations, and checks current
ABI/image/descriptor/emitter authorities and tracked ABI fixtures against HEAD.
It prints a JSON receipt to stdout; build status goes to stderr. The retained
[receipt](../measurements/74f-same-frame-deopt.json) identifies the baseline,
toolchain, source hashes and unchanged compatibility authorities.

The [implementation](../../rust/birnam-id/src/runtime/deopt_experiment.rs) uses
the actual registered-entry path, descriptor-v2 validator, native driver, ordinary
Machine sends, GC and FrameView operations. Only the optional tier selection
hook and fixture compile under `cfg(test)`. The ordinary Runtime structure,
generated-code interface, descriptor layout, and image ABI are unchanged.

| Case | Candidate entries | Result |
| --- | --- | --- |
| Canonical | optimized PC 0, optimized PC 1 | 28; two reads instead of baseline's four |
| Invalidation before entry | baseline PC 0, baseline PC 1 | 28; optimization never entered |
| Mutation then restoration | optimized PC 0, baseline PC 1 | 28; epoch change invalidates even when the original binding state is restored |
| Custom lookup | optimized PC 0, baseline PC 1 | 28; custom lookup executes, with baseline-equivalent call count |
| Native Machine read override | optimized PC 0, baseline PC 1 | 28; override observes both subsequent reads |
| Replace active method | optimized PC 0, baseline PC 1 | active call returns 28; next call returns replacement's 999 |
| Precise GC | optimized PC 0, baseline PC 1 | dead control allocation collected; carried 14 and its alias survive |
| Nested registered entry | optimized PC 0, baseline PC 1 | nested call returns 14 with two active pins; outer call returns 28 |
| Missing eligible recovery PC | optimized PC 0, baseline PC 1 | same Frame/PC remain valid; no optimized entry at the unmapped PC |
| Different descriptor with matching numeric identity | baseline PC 0, baseline PC 1 | no candidate entry; exact descriptor allocation is required |

Every row compares independently executed baseline/candidate effect traces and
answers. The expected trace is also asserted independently: argument evaluation,
body-before, effect, body-after (with one nested entry/return in that case).
Both visits retain the same generational Frame handle and original descriptor
identity; carried slots retain exact alias identity; the view reports PC 1 after
recovery and becomes stale after return. Every forced recovery has one optimized
entry, one deoptimization and one baseline entry, with three reads total.

Two further runs each reuse one Runtime for 128 trials (canonical and forced
invalidation). All 256 finish with zero active/acquired/continuation pins, zero
live Frames, no retained tier record, and balanced descriptor references.
Debug and release produced the same 11 receipt records.

Release compilation initially folded structurally identical self-referring
fixture thunks, causing registration to reject a duplicate native address.
Distinct `black_box` tags preserve the required thunk identities; the complete
matrix was then rerun in both profiles. This was fixture construction, not a
waiver of registration identity checks.

Validation on the final implementation:

- `python3 scripts/same-frame-deopt.py`: all ten matrix cases and 256 repeated
  trials passed in debug and release, with identical observations.
- `CARGO_TARGET_DIR=.cargo-target mise run verify`: workspace tests (including
  the experiment), native-prefix/lifecycle checks, two independent cold-source
  builds, and relocated runtimes passed.
- The retained receipt matches the experiment sources; ABI/image/descriptor
  authorities and tracked ABI fixtures have no changes from `5f38ce3b`.
- Design links, Python syntax, fixture formatting and `git diff --check` passed.

## Conclusion and boundary

The experiment establishes same-frame recovery at existing committed StepMethod
boundaries without the 74E exports or an ABI cut. Canonical Frame storage makes
the recovery mapping an identity mapping; no Frame reconstruction is needed.
The global epoch deliberately over-invalidates, and an invalidated trial stays
disabled until its scope ends. These are bounded experiment policies.

This does not establish speedup: removed-read counts prove candidate activation,
not application throughput. It also does not implement compiler-generated tier
admission, dynamically unloadable optimized code, mid-activation exits, register
recovery, inlined frames, eliminated objects, arbitrary FFI unwinding, or image
persistence of an optimized tier.

The next bounded investigation would generate the paired entries from one
CodeProgram and validate their shared PC/live-slot contract. Dynamic-code leases
and artifact admission need explicit treatment before promoting such a tier.
