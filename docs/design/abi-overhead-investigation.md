# ABI overhead: measured owner-message costs

Measurement-tool note (2026-10-28): the Operand/Outcome profiling runner and its
disconnected dispatch counters are retired. Commands below using that runner are
historical; use each report's measured revision to reproduce its results. Current
Oop checks include `scripts/oop-id-slice-bench.py` and the
`integer_performance` test. This note does not change the reported measurements.

The consolidated [runtime and ABI simplification plan](runtime-and-abi-simplification.md)
classifies these mechanisms by necessity and tracks the proposed removal/investigation work.

The expensive part of these crossings is the message machinery around carriers:
permissions, signature owners, result construction and invocation/capture setup.
Local Object-only incoming adaptation did not produce extra Id sends or a meaningful
CPU difference in its matched control. Outgoing bridges are expensive individually,
but their named subtrees account for about 7.15% of the successful public addition
chain's sends. That is a message fraction, not a CPU share or all possible ABI work.

No production optimization is introduced by this investigation. The optional Id
census and diagnostic fixtures were extended; native target calls remain Id sends.
The installed prefix and startup images were not modified.

## A single outgoing Integer read

The control uses an exact Integer and the captured signature `"Wo"`: optional
native Word result, one Object argument. The direct control asks the Integer owner
for `wordOf:` and receives a native carrier. The authored control invokes Execution's
bridge and receives a boxed Word. Both values are checked, but their result carrier
and wrapper contracts differ. This is a cost contrast, not an equivalent replacement
or an optimization regression test.

| Part of authored read | Id sends |
| --- | ---: |
| Vector-omission permission, including ObjectModel/GC subchecks | 30 |
| String `tryPrimitiveBytes` signature access and its owner checks | 20 |
| Word `fromBits:` construction and its allocation/root checks | 19 |
| Actual Integer `wordOf:` subtree | 7 |
| Five intern requests and their symbol queries | 10 |
| Two epoch queries, one binding batch and bridge entry | 4 |
| **Outgoing bridge subtree** | **90** |
| Wrapper dispatch, capture retrieval, caller roots and cleanup outside bridge | 67 |
| **Complete authored wrapper** | **157** |

The direct native carrier read uses 7 sends. A separate two-second, 100 ms batch
probe produced 16 paired samples, with medians of 0.307 microseconds for that read
and 10.921 microseconds for the authored boxed wrapper. These include harness loop
costs and normal-GC work. They are local diagnostics, not a language-wide throughput
claim. The initial 25 ms batches could not meet the reporter's one-millisecond CPU
floor on the fast baseline; they were reported inconclusive rather than used anyway.

Signature text parsing is not twenty sends. The twenty sends obtain the signature
through its String/Bytes owners and establish primitive access. The actual short
signature check is local Rust work. Likewise, carrier conversion itself is not the
thirty-send permission chain. The argument Vector is already omitted in this case;
no Vector or Block is allocated by the crossing.

## Incoming adaptation and round trips

All controls use compiler mask 31 on both sides. Result checks and native signature
inspection confirm the Object control is genuinely untyped versus adapted. Word and
Bytes controls retain the same outgoing wrapper; only the leaf switches between a
native echo and an authored carrier adapter. The native echo returns the supplied
carrier through Id and reads no Object payload. Byte borrows remain caller-owned for
its invocation. These are deliberately different leaf contracts, not proposals to
replace arbitrary authored code with a native echo.

| Control | Baseline sends | Authored/adapted sends | Additional Id allocations per call |
| --- | ---: | ---: | --- |
| Object-only incoming method | 12 | 12 | None |
| Integer read to Word result | 7 | 157 | One Word |
| Integer read, then construction from those bits | 26 | 276 | One temporary Word; both construct one Integer |
| Word echo leaf inside the same outgoing wrapper | 158 | 206 | One additional Word |
| Five-byte echo leaf inside the same outgoing wrapper | 210 | 265 | One additional Bytes object |

The ten-second group run measured about 1.33 microseconds for both Object-only
variants; there was no meaningful difference. Word echo medians were 11.37 versus
14.84 microseconds, and Bytes echo 14.65 versus 18.41. These short runs isolate
specific costs; their ratios are not full-application speedup predictions. Id
allocation counts exclude Rust allocator activity. The incoming path still clones
immutable signature storage and uses temporary Rust vectors, including a copied-
argument inventory and normalized arguments; those remain potential CPU cleanup,
but the Object-only result does not prioritize them over message cascades.

Incoming signatures are parsed when Methods are registered and stored on the
Execution-owned Callable. They are not repeatedly reparsed on invocation. Incoming
Words require current Word factory construction before the authored body, and a
Word result requires current `bits` afterward. Incoming Bytes are copied before
factory callbacks, wrapped through their owner, and copied on return. Integer's
successful arithmetic bridges use `"Wo"` and `"ow"`; byte copying is not on that path.

## Connection to public addition

The sixteen-addition chain performs three successful ABI bridge attempts per
addition: two Integer-to-Word reads and one Word-to-Integer result construction.
The warmed candidate census partitions them as follows:

| Named outgoing ABI subtrees, per addition | Sends |
| --- | ---: |
| Vector-omission permission chains | 100 |
| Signature access | 60 |
| Two Word result constructions | 38 |
| Two Integer reads, Word input decoding, Integer construction | 38 |
| Interning | 30 |
| Epoch fences, binding batches and bridge entries | 12 |
| **Total named outgoing bridge sends** | **278** |
| Remaining sends outside those named subtrees | 3611.75 |
| **Total** | **3889.75** |

The first post-collection call measured 278.3125 bridge sends out of 3890.0625.
Warming after collection removed only 0.3125 sends per addition; it did not erase
this overhead. The small standalone controls had identical post-collection and
warmed counts. In particular, ObjectModel and GC still revalidate allocation
permissions and binding facts inside `canOmitAllocationFor:`. The Vector method-
meaning cache does not remove those fresh queries.

These named regions exclude incoming adaptation that has no separately named Id
message, and exclude ordinary Word boxing performed by other primitives. Do not
interpret the outside bucket as having no ABI-related work. Region membership is
based on nearest selector ancestry, not foreign method-context inspection or a
claim that a selector uniquely identifies an owner. Regions are disjoint and sum
to the census. Inclusive subtrees overlap unless an explicit parent/child cut is
chosen; the bridge tables use direct children plus the bridge entry.

A separate two-second sampling profile supports the message findings: the highest
reported leaf bucket is Id dictionary lookup, followed by hash/dispatch, memory
operations and root-registry work. `incoming` appears above most of the call graph
because it encloses the entire authored wrapper. Its inclusive sample count must
not be mistaken for exclusive carrier-conversion CPU time. The profile cannot
assign every memory copy to ABI payload bytes.

## Next reductions worth testing

1. **Batch permission facts at their owners.** The 30-send permission chain is the
   largest bridge component. Share callback-free lookup/allocator validation for
   Vector and its node while retaining fresh GC phase and permission decisions.
   Changes without an epoch increment must still force fallback. Do not cache a
   positive permission verdict or skip a replaced provider's effects.
2. **Let signature owners admit a compact fixed plan.** Avoid repeatedly walking
   the String/Bytes permission chain for a proven fixed signature. Preserve current
   getter/lookup meanings, owned-byte lifetimes and dynamic/invalid-signature
   fallback. Caching a borrowed pointer or a parsed value without those dependencies
   would be incorrect; parsing the same three bytes faster is a small target.
3. **Prove when a temporary Word result can stay a carrier.** The round-trip control
   constructs a Word only to extract its bits for Integer construction. Removing
   it needs a compiler region proof covering nonescape, identity, root/Frame and
   allocation observations, callbacks and recovery. It must keep both Integer
   owner messages and authored signed policy. This is a larger obligation than
   replacing a local vector with a stack buffer.

Object-only incoming buffer/clone cleanup remains a bounded fallback opportunity,
but the measured control gives it lower priority. These ABI reductions alone will
not remove the majority of addition's message count. Block construction and
capture/control costs remain the larger overall target.

## Reproduction and evidence

```sh
python3 scripts/microbench.py --case abi-audit --counts-only
python3 scripts/microbench.py --case abi-audit --seconds 10
python3 scripts/microbench.py --case abi-audit-read --seconds 2 --batch-ms 100
python3 scripts/microbench.py --case abi-audit-read --seconds 2 --profile candidate
python3 scripts/microbench.py --case public-integer-bindings --counts-only
```

The permanent receipt is `docs/measurements/abi-overhead-20261007.json`; raw data and
sampling output are under `/tmp/birnam-abi-audit/`. Timed work used the ordinary Id
build; counters used an isolated metrics Id. Output, allocation and root checks
passed, each region/edge/subtree call total equals its selector census, and inputs
were stable during all retained runs. Workspace compilation, the census nesting/
reset/epoch regression and four reporter tests passed. No long benchmark, startup
image rebuild or idle-machine requirement was used.
