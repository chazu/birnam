# 74L — Guard the existing default lookup implementation

Status: implemented and correctness-verified, 2026-09-11. The combined 74J/74K/74L
candidate meets the declared timing criterion against the original baseline
on the 1,024-iteration warm fixture, without an above-spread regression. The
incremental 74L comparisons are inconclusive. The protocol below preceded implementation and measurement.
The 74K three-prefix comparison completed with every row inside its observed
spread. No independent speedup is claimed for PC specialization.

## Problem and change

Even an ordinary send using the canonical lookup primitive recursively sends
`lookup` through the metacircular resolver. 74J eliminates this work for
canonical Machine slot sends, but other sends still pay it. Optimize the
default lookup *implementation* while preserving every semantic lookup.

After the existing metacircular base case, and only while a surrounding
dispatch-read lease is active, check that the receiver's vtable uses the
canonical metavtable, that the canonical metavtable uses itself, and that its
direct `lookup` entry is the actual floor lookup function. Check resumable
registration independently of dictionary mutation. A registered lookup carrier,
custom lookup or private metavtable must retain ordinary resolution and effects.

When eligible, run the same private dictionary lookup used by that canonical
primitive. Resolve the current dictionary every time: do not cache method
targets, results, receiver values or native-code ownership. Keep ordinary
resolve-and-pin classification, invocation and DNU handling after lookup.
The outer dispatch-read lease still fences retirement. At a saturated mutation
epoch, recheck the cached root conditions on every call.

This changes neither the IdApi table nor any exported native interface, language
selector, primitive meaning, Frame layout, registration record or image format.
Each result remains expressible by the original ordinary Id send. In particular,
an effectful custom lookup cannot be skipped or called an extra time.

## Acceptance

Compare enabled/disabled lookup in fresh runtimes. Prove hits independently of
answers; cover native replacement, inheritance, shadowing/removal, DNU arguments,
private metavtables, custom lookup, saturation and resumable lookup registration
whose address still equals the floor function. Retain the prior real native
mutation, same-Frame, precise-GC, native-retirement and full-prefix gates.

Reuse 74K's fixed portable benchmark, sizes, seven groups, warmup, forwarding
lookup fallback, ordinary-send control, host gates and spread criterion. Compare
the preserved 74K prefix with the updated candidate and the original baseline.
This is a new implementation revision, not a reinterpretation of 74K's results.
Record code-size and setup cost alongside steady execution. Any above-spread
candidate regression prevents promotion; no gain is claimed from a result within
the observed spread.

## Implementation and verification

`Runtime::bind_active` retains its existing metacircular base case, then checks
the private `guarded_default_lookup_eligible` predicate. A hit calls the same
`lookup_method_active` implementation as the canonical primitive. A miss follows
the existing lookup send. The cached value covers only the canonical root
conditions at a dictionary epoch; every target lookup still reads the current
dictionary and inheritance chain. Receiver metavtable identity and resumable
registration are checked independently on each attempt.

Two focused tests pass in debug and release. Eight enabled/disabled cases cover
canonical dispatch, method replacement, inheritance changes, shadow removal,
global lookup override, a private metavtable, epoch saturation and missing-method
argument forwarding. Lookup effects and original argument identities agree;
guard hits are observed independently. A separate test registers the canonical
lookup function address as a resumable carrier without mutating the dictionary,
checks invalidation and restoration, and proves that the published carrier
actually handles the next lookup.

The complete Id suite passes with 131 tests and seven existing ignored tests.
The [full verification receipt](../measurements/74l-verification.json) records
workspace tests, normal native prefixes, independent cold archives and relocated
runtimes. The timing runner additionally reruns all twelve native-tier pairs in
debug and release before measuring the uninstrumented prefixes.
The final 74F/74G/74H recovery and native-unload runners pass in both profiles;
74I's twelve fresh C-client processes agree against ordinary debug/release floor
builds. The three guarded Machine tests also pass in release. Their receipts
and source hashes are linked from the final verification receipt.

Run the bounded correctness checks with `mise run native-tier-test`. Reproduce
the three-prefix comparison using `scripts/runtime-native-tier.py --design 74L`
with explicit `--baseline`, `--slots` and `--candidate` prefixes. Here `--slots`
names the preserved 74J/74K prefix; the receipt records these labels explicitly.
Use a distinct `--output` for every new measurement attempt.

## Measurement result and limits

The [retained receipt](../measurements/74l-guarded-default-lookup.json) contains
all seven groups of process medians and setup costs, prefix/source hashes and
interval samples. Every host gate passed, with median idle CPU of 66.54–70.16%
and no low-idle intervals. The candidate matches the current source manifest;
all three floor export inventories contain the same 25 symbols. The API,
registration, descriptor, ordinary emitter and image authorities are unchanged.

Times below are microseconds; warm/fallback values are medians of seven
fresh-process medians, each using five timed repetitions:

| Work | Original baseline | 74J + 74K | 74J + 74K + 74L | Candidate vs original |
| --- | ---: | ---: | ---: | --- |
| Warm, 1,024 iterations | 42,698 | 37,129 | 35,322 | 17.3% improvement, qualified |
| Fallback, 1,024 iterations | 44,533 | 45,501 | 44,719 | Inconclusive |
| Warm, 4,096 iterations | 213,785 | 198,123 | 189,363 | Inconclusive |
| Fallback, 4,096 iterations | 213,075 | 222,010 | 221,921 | Inconclusive |
| Ordinary-send control | 4,793,237 | 4,546,193 | 4,387,164 | Inconclusive |

For the qualified row, the median improvement is 7,376 us, exceeding both
observed ranges (4,041 and 4,720 us). There is no above-spread candidate
regression. All five incremental comparisons against the 74J/74K prefix remain
within variation. The 74J/74K prefix also improves against the original baseline
on the smaller warm fixture in this run. Consequently the evidence qualifies
the combined candidate, but does not isolate a benefit attributable to 74L or
PC specialization. It neither changes 74K's earlier inconclusive result nor
qualifies 74J's rejected longer E1 measurements.

For 1,024 iterations, median materialization cost is 591/688/701 us across the
three prefixes; first-call cost is 19/19/19 us, and the 32-call warmup costs
1,284/1,662/1,594 us. At 4,096 iterations those setup medians are 565/599/609 us,
18/19/18 us and 1,240/1,465/1,410 us. The native-tier compilation occurs during
warmup. These are reported costs, without a separate significance claim.

The final floor library is 1,334,464 bytes, 112 bytes smaller than 74J/74K and
602,704 bytes larger than the original baseline. The added compiler and native
ownership mechanism are a real footprint cost. The implemented tier covers
eligible portable Blocks and same-Frame recovery at existing PC boundaries;
inlining, unboxing, repeated-read caching and multicore execution remain outside
this promotion.

The subsequent [74M isolation](74m-native-tier-performance-isolation.md) finds
no independent native-tier speedup beyond variation, and identifies a 420 us
warmup cost in the smaller chain. Native entry counts in this original fixture
are accounted for by warmup before candidate invalidation. The combined result
above remains valid; it does not demonstrate a steady PC-specialization gain.

[74N](74n-native-tier-dependency-revalidation.md) fixes unrelated dictionary
invalidation and repeats the isolated comparison. Native execution now spans
the original fixture's timed batches, but the seven groups still establish no
independent execution or memory benefit beyond variation.
