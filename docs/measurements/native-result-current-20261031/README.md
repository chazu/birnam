# Ordinary native results on the current runtime

Candidate based on `fdd658c6`, 2026-10-31. **Not applied to production.** The
independent-drive policy decision described below is still pending. This updates
the [earlier proposal](../native-result-20261031/README.md) after the intervening
buffer, root reporting and synchronous-return changes.

## Behavior and implementation

Native-signature adapters validate arity and required arguments, normalize inputs,
invoke the captured body, convert its result and notify configured failure
observers. These operations and their order remain. Returning a Method or unmarked
Fault object remains an ordinary data return. A deferred `v` signature continues
to schedule its selected branch outside the synchronous body boundary.

For ordinary results, the candidate changes:

```text
Before: open private drive/answer -> convert -> invoke body -> convert result
        -> allocate reply work -> record private answer -> close drive
After:  convert -> invoke body -> convert result -> return
```

Body invocation retains its existing explicit Method message and synchronous
boundary. It still allocates its argument Sequence and entry work. This is not the
rejected plain-Boolean replacement and does not discard native arity checks.

Two native declarations use the existing ordinary calling convention, for bound
and unbound adapters. They share the existing conversion implementation. The old
continuation declarations remain usable, and declaration/literal ordinals are
preserved by appending the new entries. A substituted deferred-signature context
is rejected by an ordinary entry before conversion or body effects. Id, its ABI,
numeric policy and payload interpretation are unchanged. No registry or service
is added.

The synchronous call scope retains inputs and results across GC and callbacks;
its cleanup makes no callback between returning the result and Id receiving it.
Deferred branch handling retains the existing continuation and failure observer.
This changes native execution mechanics, not Birnam ownership or borrowing rules.

## The outstanding behavior difference

A replacement Word conversion factory attempts an independent Id drive. The
refreshed probe now supplies real work and checks that it executes exactly once,
rather than merely attempting an empty drive.

| Invocation context | Baseline | Candidate |
| --- | --- | --- |
| Native host call, no preexisting drive | `driveActive`; work does not run | Work runs once |
| A drive is already running | `driveActive`; work does not run | Same refusal |

The baseline's first refusal comes from the adapter's own private drive. The
candidate removes that drive. This is observable to a callback, so passing the
other tests is not authorization to apply it. The user has been asked which
behavior is intended; no response is inferred from elapsed time or continued work.

## Measurements

Only Execution differs between the two copied source-built prefixes. Release
arithmetic measurements use baseline/candidate/candidate/baseline order with no
concurrent builds or tests. Each median has ten samples of 1000 calls. Inputs,
selectors, compilation and explicit pre-sample collection are outside timing.

| Automatic GC enabled | Baseline | Candidate | Managed allocations |
| --- | ---: | ---: | ---: |
| Integer addition | 28.24 µs | 26.35 µs | 40 → 30 |
| Exact Error check | 5.39 µs | 5.04 µs | 5 → 3 |
| Signed addition/overflow helper | 7.35 µs | 7.03 µs | 12 → 10 |

Addition takes about 6.7% less time and allocates 25% fewer objects in this
comparison. With automatic GC disabled diagnostically it measures 28.09 versus
26.51 µs. These are small-operand measurements, not an application speedup or a
claim that Integer arithmetic is now adequately fast.

The resumable inventory comparison matches all six fixtures in each process but
shows no useful improvement. Dense median is 339.62 → 340.24 ms; real-module
median is 792.15 → 805.56 ms. These medians combine fourteen samples per variant
in the same ABBA order. The candidate does not qualify that migration.

## Validation and status

The dedicated return probe measures five allocations on baseline and three on the
candidate. It checks returned Methods, wrong arity, required-null rejection,
failure observation during GC, the body suspension barrier, actual independent
drive work, and refusal of overlapping drives. Its baseline failure is the
proposed independent-drive expectation; the observed baseline allocation count
is recorded separately.

The candidate passes workspace all-target/all-feature checking and 320 default
workspace library tests. The eight focused integration targets cover native
incoming conversion, native boundaries, continuation transparency, Execution
Methods, Integer values, Integer operation objects, Integer box observability,
and the new return probe. The three feature-specific compiler memory-metric
tests from earlier 323-test receipts are not part of this default-feature run.

Seven further integration targets pass: Execution Machines, full image
roundtrip/resaves, image native refusal, image slot values, image snapshot stamps,
root batches and root reuse. This gives 15 passing integration targets, including
the new return probe. The source and native ABI were reviewed separately from
those tests; the independent-drive behavior difference remains unresolved.

[candidate.patch](candidate.patch) contains the implementation and regression
fixtures. It applies to the recorded baseline and remains experimental evidence,
not an instruction to change production behavior. [result.json](result.json)
records source/library hashes, commands, measurements and validation scope.

The isolated source and copied prefix are under
`.cargo-target/measurements/native-result-current-20261031`. Main retains its
existing behavior. No full-source rebuild, installation, publication, accepted
policy change or completion of the overall runtime goal is claimed.
