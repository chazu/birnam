# Tail-call regression: routing diagnosis

Later continuation: the [compiler/Execution continuation integration](oop-continuation-id.md)
now passes the original regression. This document records the earlier diagnosis;
the [isolated shared-entry experiment](oop-entry-seam-experiments.md) was not the
chosen implementation.

2026-10-24. Diagnosis complete; production repair is not implemented. The user
requested determination of the cause, so no runtime fix or architecture change
was attempted. Id and the tracked runtime sources are unchanged.

[Measurements, traces and diagnostic patches](../measurements/oop-tail-diagnosis-20261024.json)
bind this result to `4e7b77b1` and its runtime implementation from `f6e98c47`.
The private probe builds are under `.cargo-target/diagnose-tail-4e7b77b1/`.

## Confirmed cause

The driver uses whether lookup is proven callback-free to decide whether it can
recognize a resumable callee. That condition belongs to its preliminary lookup
optimization; it does not determine whether the Method actually selected is
resumable. In the custom case the driver sets `selected` to null without obtaining
the lookup result. Registration recognition is skipped, even though the Method
returned by the subsequent ordinary send is registered.

The exact path in `rust/birnam-execution/src/resumable_runtime.rs` is:

1. `drive` decodes `TailCall`; `tail` is true.
2. `pure_lookup` declines the custom lookup. `drive` substitutes null for
   `selected`, so `registration` has no Method to recognize.
3. `drive` enters its synchronous boundary and performs ordinary Id send.
4. Lookup returns the original registered source Method. Its `entry` function
   finds its registration and calls `start_registered`.
5. An active Machine exists, so `start_registered` begins a nested drive and
   calls `push(..., tail=false)`.
6. That push retains the old source frame and adds another. The outer driver's
   eventual `if tail { finish(...) }` runs only after the recursive call returns,
   too late to reuse the frame during recursion.

The default path reaches `push(..., tail=true)`, which calls the retained
`replace_current_with_entry` algorithm. It keeps one source frame plus the run
adapter. The custom path keeps one source frame per execution plus the adapter:
exactly 9 frames for 8 executions and 25 for 24.

## Hypotheses and observations

Two original-process replays produced identical failures. Then isolated builds
changed one relevant condition at a time; none replaced the production library.

| Probe | Custom lookup: 8 calls | Custom lookup: 24 calls | Conclusion |
| --- | ---: | ---: | --- |
| Original runtime, two runs | 9 | 25 | Stable reproduction |
| Tagged routing/frame trace | 9 | 25 | Instrumentation preserves the symptom |
| Remove only lookup-time GC | 9 | 25 | GC is not responsible |
| Also remove lookup bookkeeping during the probe | 9 | 25 | Forwarding lookup alone triggers the route |
| Diagnostic recognition of custom tail targets | 2 | 2 | Returned source Methods are registered; tail transfer works |
| Original runtime after all probes | 9 | 25 | No diagnostic change leaked into production |

The original trace for eight executions shows eight decoded tail calls with
`pure=false`, `registered=false`, and a null substituted selection. There are
seven source re-entries and seven growing pushes (`2 -> 3` through `8 -> 9`),
all with `tail=false`. The final native stop returns the requested value.

The diagnostic recognition control instead finds seven registered source targets
and one native stop. The seven recursive transfers use `tail=true` and each keeps
`2 -> 2` frames. Thus the compiler's tail marker, the registry and the frame reuse
algorithm work. The error is in choosing the fallback before recognizing what
custom lookup selected.

## Scope of the result

This is a regression in the migration's routing introduced with `f6e98c47`, not
an intended language rule. The retained pre-migration `machine.rs` classifies the
selected entry after lookup independently of whether lookup was custom. The
preserved Design 60E/60F contract requires the same behavior.

The diagnostic forced-recognition change is **not a valid repair**. If its
preliminary custom lookup returns a native target, the existing ordinary-send
fallback can execute lookup again. Likewise, changing every nested push to a tail
replacement would break genuine native callbacks. These probes establish cause;
they do not authorize weakening either dispatch or callback semantics.

A production repair must retain the actual selected Method and original Call/
TailCall context through one lookup, recognize registered source code, and enter
native code without another lookup on that selected Method. It must keep the Id
contract fixed. This diagnosis does not establish that an ownership redesign or
new Id operation is necessary. The earlier ownership-blocker framing was premature.

The missed regression was a composition gap: custom-lookup output/effect checks
and default-lookup tail-frame checks passed separately. The mandatory combined
test now protects the required behavior. The production gate remains red until
a repair satisfies it together with the native-dispatch and single-lookup tests.
