# Reuse lookup results for an unchanged dictionary graph

Runtime baseline `9a053b1f`, 2026-10-31. The separate test-hook repair is
`1313c6b6`. This replaces Id's dictionary-position hints; it does not apply the
pending ordinary-result adapter proposal.

## Required behavior and sequence

Ordinary sends must observe current method lookup, delegation, replacement and
custom lookup. An entered Method retains its selected implementation; a later
send observes changes. Explicit captured invocation remains separate, and returning
a Method must not execute it. Cached lookup must not keep dead objects or code alive.

Before:

```text
check current receiver/metabehavior and default lookup
  -> walk parent dictionaries
  -> validate a position hint against each current key, or search its dictionary
  -> enter the selected Method using its current data
```

After:

```text
check current receiver/metabehavior and default lookup
  -> reuse an exact receiver-vtable/selector result at the current mutation epoch
     or walk the current dictionaries and record the result
  -> enter the selected Method using its current data
```

The bounded cache contains 256 entries, including absent results. It replaces the
old hints rather than adding another lookup layer. Keys and values are weak
native bookkeeping; the existing dictionaries and call scopes retain live objects.
No public object representation, Id operation or native ABI changes.

The dictionary owner already controls edits, parent changes, reassociation and
retirement. Those paths advance the existing mutation epoch. Once that counter
saturates, invalidation clears entries explicitly instead of relying on epoch
inequality. Retirement invalidates results without changing the existing binding
epoch behavior. The mutable record accessor is now private to the dictionary owner.

Custom lookup is still selected before the ordinary dictionary path. Method
code, context and owner are not cached here: mutating admitted Method data without
a dictionary edit does not make entry use stale data. No callbacks occur during
the cache check or dictionary walk.

On this 64-bit build, the cache occupies 8192 bytes instead of the old hints'
4096 bytes. There are no per-send allocations. The production vtable module grows
by ten lines, including shared invalidation, while removing the per-dictionary
position-hint path. The measured benefit below justifies that bounded state cost.

## Final measurements

Only Id differs between copied source-built prefixes. Builds and tests are stopped
during baseline/candidate/candidate/baseline comparisons. Arithmetic uses ten
samples of 1000 calls per variant and GC mode; resumable inventory uses fourteen
samples per fixture and variant. Compilation, inputs, selectors and explicit
pre-sample collection are outside arithmetic timing.

| Automatic GC enabled | Baseline | Changed Id | Time reduction |
| --- | ---: | ---: | ---: |
| Integer addition | 28.10 µs | 26.36 µs | 6.2% |
| Exact Error check | 5.41 µs | 4.94 µs | 8.6% |
| Dense authored inventory | 323.81 ms | 310.61 ms | 4.1% |
| Real-module authored inventory | 750.78 ms | 725.46 ms | 3.4% |

Addition still allocates 40 managed objects. All six inventory fixtures match in
every process. These results do not establish an application speedup, adequately
fast Integer arithmetic or a production-ready inventory migration.

## Validation and separate architecture review

Workspace all-target/all-feature checking and 321 default library tests pass.
Fifteen integration targets pass, including full image roundtrip/resaves, image
refusal/slots/snapshot stamps, mutable query/primitive Methods, custom lookup,
continuation transparency, construction purity, Machine execution, Integer policy
and replacement, and root lifetime. The independent C continuation suite passes.
The full Oop gate passes, including the independent C and vendored TinyCC callers,
native-owner checks, GC/finalization/retirement, direct dispatch, compiled controls,
artifacts and 177 authored task-control checks.

The dictionary tests retain comparison with an independent linear lookup across
replacement, removal, inherited resolution, parent changes, slot shifts and
retired/reused records. The old forced-position-corruption check is replaced by
exact-key collision checks, since positions are no longer stored. A new test warms
both absent and inherited results and checks edits and retirement at a saturated
epoch. Passing those tests is separate from auditing every graph mutation path,
the cache's lack of GC roots, and current Method entry.

### Repair to the broader test gate

The initial full gate failed a suspension assertion on both unchanged Id and the
candidate. Its fixture intercepted `callSite:`, while generated code now offers
the distinct bulk `prepareCall:selector:` operation. The hook missed the intended
source call. The repair intercepts that actual operation and retains the existing
suspension, capture, resumption, effect-count and boundary assertions.

For boundary tests, the controller prepares operands through public call-plan and
slot messages before publishing one boundary action. Forwarding ordinary call
preparation first would already transition the frame and make a second call invalid.
No test reads private Machine or action payloads. The updated numeric scenario
passes on unchanged Id; the full gate then passes on the candidate. This repair
does not make a bulk operation forward through lower-level operations.

[result.json](result.json) records source/library hashes, commands, measurements,
checks and the retained baseline failure evidence. Raw files are under
`.cargo-target/measurements/lookup-cache-20261031`. This uses a rebuilt Id over an
existing source-built prefix; it is not a new installer/source-build qualification.
No user-prefix installation, push or completion of the overall runtime goal is
claimed.
