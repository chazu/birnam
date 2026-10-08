# Retire the general outbound ABI bridge

Implemented 2026-10-07. All seven remaining production call sites have been
migrated to fixed-shape Id messages. The signature-based outbound API and its
compiler optimization are deleted. Incoming authored-method ABI signatures and
Alien foreign-call signatures remain separate live interfaces.

## Remaining callers and replacement

| Production caller | Fixed Execution operation |
| --- | --- |
| Integer `fromWordResultOf:selector:` | existing `objectResultOf:selector:wordArgument:` |
| Word `asInteger` | existing `objectResultOf:selector:wordArgument:` |
| Boolean native setter | `discardResultOf:selector:wordArgument:` |
| Bytes native-result constructor | `bytesResultOf:selector:resultFactory:` |
| Bytes `asString` | `objectResultOf:selector:byteArgument:` |
| Boolean true/false printing (two sites) | `objectResultOf:selector:byteArgument:` |

Seven is a count of static source locations; these operations may execute many
times. Targets, selectors, supplied factories and actual values remain dynamic.
Source constants holding `ow`/`ob` signature Strings and unused factory captures
are removed. Boolean/signed numeric/formatting policy remains in Birnam.

## What was removed

- `sendNativeTo:selector:arguments:signature:resultFactory:` and `tryNativeArguments:`
  are no longer installed by Execution. No old-call compatibility shim exists.
- The outbound signature interpreter, arbitrary argument collection/count loop,
  conversion-kind loop, normalized-operand arrays and Vector/signature permission
  chain are deleted. Fixed functions convert one argument or obtain zero-argument
  results directly; the necessary Word/Bytes copying and conversion remain.
- NativeBackend's `abi_arguments.rs` planner and emitted ABI-literal fast/fallback
  branch are deleted. Bit 2 is retired and rejected; default optimization mask is
  29, retaining bits 1, 4, 8 and 16. Other optimization features keep their meanings.
- Vector's `canOmitNativeArguments` and cached proof state are deleted. String and
  Bytes `tryPrimitiveBytes` helpers are deleted: their only production consumer
  was the removed outbound optimization. Ordinary `bytes` getters remain.
- The obsolete Execution `bindingProofBatches:` toggle, unused guard entry point
  and reentrant separate-query fallback are deleted. Atomic Id binding checks
  still serve capture borrowing and Word-helper reader admission, where they are
  necessary to decline borrowing from replaced/custom readers.
- Tests specifically validating retired bridge/vector interception and permission
  behavior are removed. Native-value, byte-mutation and incoming-adapter tests are
  migrated to actual supported interfaces.

Old artifacts containing removed calls or requesting retired masks must be
explicitly rebuilt. Images containing machine code are not silently recompiled
at startup. Rust needed for the cold compiler/identity/value foundation remains.

## Preserved contracts

Every actual target call uses current Id dispatch. Word arguments are obtained
through their current `bits` messages. Byte arguments are copied from the current
owner's `bytes` result before target callbacks, which can mutate/reallocate the
original buffer or collect. Returned byte spans are copied before a factory
callback. No foreign payload/context inspection, stored executable dispatch,
new Id ABI field or new loaded service module is introduced.

All Object inputs are scoped before dependency sends. Actual target/getter/factory
failures keep their native status/detail. Object results normalize nullable Object
to language nil. Optional Word results distinguish native Unit from numeric zero.
Discarding a successful result accepts any native carrier and returns language nil;
actual failures are propagated. Byte-result constructors retain the explicitly
supplied factory, including derived/custom factories.

This is an explicit interface/source redesign. Effects and failure points of the
retired signature/Vector algorithms are not manufactured in the new implementation.
Live operations retain their actual results, effects and defined order. The new
interfaces are ordinary replaceable Id methods.

## Validation

Passed affected Execution/NativeBackend/Bytes/String/Vector Rust tests and workspace
all-target compilation. NativeBackend tests cover all sixteen valid masks and
reject the retired bit and out-of-range requests. Production integration checks pass:

- Byte argument mutation/reallocation and precise collection inside the target;
  the target still reads the original copied data.
- Byte-result factory mutation of the original byte owner and collection; the
  factory still receives the pre-callback result snapshot.
- Explicit custom factory selection, factory/target failures, nullable/wrong result
  kinds, invalid input arities/carriers and discarding native Bytes/Unit results.
- Production Boolean setters, Bytes native-result wrapper and Integer word-result
  wrapper; numeric zero is distinct from an absent optional result.
- Integer signed-boundary/error/Float oracle, Boolean policy, incoming authored
  carrier signatures, remaining fixed-call callback/mutation/lifetime tests,
  replaced capture readers and Word-helper admission.
- Captured Cells across wait/resume/GC and cancellation/cleanup at quanta 128 and 1.
- Removed outbound selectors have no bindings on Execution's behavior.

No new formal implementation proof is claimed; the existing scoped-input/borrow
rules remain the model premises and concrete callback tests verify the new routes.
The full image suite, 171-check task suite and subprocess-worker image acceptance
were not rerun. No full startup performance claim is made.

The early authored diagnostic artifacts were explicitly recompiled with the
retained Rust grammar seed; production Bootstrap was restored and the seed removed.
No installed prefix or image is published by this turn. All changes remain
uncommitted at the implementation checkpoint.

## Tooling and measurements

`abi`/`abi-captured` elision benchmarks and the binding-proof toggle control are
retired. The whole-addition trace case is now `public-integer-default`, an identical
baseline/candidate protocol fixture with default mask 29. It measures census or
before/after library changes, not an optimization toggle. The analyzer still accepts
historical selector labels to analyze old receipts; runtime implementations are gone.

Read/roundtrip ABI probes use fixed typed calls versus direct Id calls. Word/Bytes
incoming probes use `ow`/`ob` incoming signatures and Object results, with native leaf
controls constructing the same result types. Historical `ww`/`bb` timings describe
different output contracts and are not directly comparable. All fifteen normal
fixtures and five ABI-audit fixtures compile and check their expected results; the
short aggregate budgets are smoke validation, not individual speed claims.

[Receipt](../measurements/outbound-bridge-retirement-20261007.json) contains hashes,
source/library identities, test records and two-second before/after observations:

| Public policy operation | Before median CPU | After median CPU | Directional reduction |
| --- | ---: | ---: | ---: |
| Bytes `asString` followed by size | 37.3 µs | 10.8 µs | 70.9% |
| Boolean `printString` followed by size | 17.2 µs | 11.4 µs | 33.9% |

These are separate active-host sessions, not paired confidence intervals. Sources
and libraries remained stable during the measured after-runs. Addition still takes
829.375 sends and allocates twenty Id Objects: its fixed calls were already migrated
in the preceding slice. No general-bridge or removed permission messages occur in
the new census; tracked Root tokens finish at zero.

```sh
python3 scripts/microbench.py --case public-integer-default --counts-only --trace \
  --output /tmp/birnam-retire-after/final-trace.json
python3 scripts/microbench.py --case semantic-bytes-as-string --seconds 2
python3 scripts/microbench.py --case semantic-boolean-print --seconds 2
```
