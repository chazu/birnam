# Finish a native body's synchronous return directly

Baseline `27c12968`, 2026-10-31. This removes a private deferred reply allocation
without changing explicit reply preparation or user continuations.

## Required behavior and sequence

The native body's result must reach its synchronous caller unchanged, including
Method-as-data and marked failures. The existing drive must remain active through
callbacks; suspension and overlapping-drive refusals remain. Explicitly prepared
work must remain inert until driven, and user reply Methods retain their normal
entry and scheduling behavior.

Before:

```text
enter ordinary native body -> allocate reply work -> drive reply work
  -> record private answer -> close synchronous boundary
```

After, when the destination is exactly the current drive's private answer:

```text
enter ordinary native body -> record private answer -> close synchronous boundary
```

The drive stores that answer reference in place of its old synchronous Boolean.
A null answer denotes an ordinary externally driven loop. The enclosing entry's
existing call roots keep the answer alive; no registry or additional managed object
is introduced. Answer recording is shared with the existing reply entry and makes
no callbacks or collections. The selected body's result is validated before this
handoff.

All other destinations retain deferred reply work. The public `answerTo:with:`
operation is unchanged, including for a private answer explicitly passed to it.
Only the automatic completion of an ordinary native body uses the shortcut. The
private answer still rejects duplicate/late delivery and clears its result on close.
There is no change to Id's ABI, payload opacity, method lookup, numeric policy or
language memory rules.

This keeps the synchronous drive; it does not implement the separate pending
ordinary-return proposal or change its independent-root-drive refusal.

## Checks and measurements

The independent C regression fails on the baseline at three allocations and passes
at two. This is an implementation-cost check, not a language allocation promise.
It also verifies Method-as-data, failure returns, collection inside the native body,
user reply deferral with a one-step drive budget, and inert explicit reply preparation.
The existing continuation suite covers late replies, native barriers, custom lookup,
interleaving and code/context lifetime.

The final release comparison used baseline/candidate/candidate/baseline order with
builds and tests stopped. Only Id differs between the copied prefixes. Arithmetic
has ten samples per variant and GC mode, each averaging 1000 calls; selectors,
inputs, compilation and explicit pre-sample collection are outside timing.

| Automatic GC enabled | Baseline | Changed Id | Managed allocations |
| --- | ---: | ---: | ---: |
| Integer addition | 29.53 µs | 28.27 µs | 49 → 40 |
| Signed addition/overflow helper | 7.58 µs | 7.28 µs | 14 → 12 |
| Nil-returning Block | 0.70 µs | 0.57 µs | 3 → 2 |

Addition is about 4.3% faster in this comparison and allocates 18.4% fewer objects.
With automatic collection disabled diagnostically, it measured 29.68 versus 29.06
µs. The allocation reduction is clearer than the small CPU-time differences.
The resumable inventory prototype changed little: dense median 340.86 → 336.18 ms
and real-module median 788.90 → 782.46 ms. Both variants matched all six fixtures.
This does not qualify the inventory cutover or establish an application speedup.

The final implementation passes the independent C continuation suite, workspace
check and 323 library tests. All 18 integration targets pass, including full image
roundtrip/resaves, callback/reentry, suspension, numeric policy, replacement and
root lifetime. The native-provider Root path also passes independently. No pending
reentry-policy decision is inferred from these results.

[result.json](result.json) records source/library hashes, commands, measurements
and validation scope. This uses the prior source-built prefix with a rebuilt Id;
it is not a new whole-source build or release qualification.

The existing arithmetic and inventory probes are reused. Raw evidence remains under
`.cargo-target/measurements/synchronous-return-20261031`. No user-prefix installation,
remote publication, or completed migration is claimed.
