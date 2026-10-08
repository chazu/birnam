# Fixed native calls without signature/container machinery

Follow-up: [complete outbound bridge retirement](outbound-bridge-retirement.md)
migrates the remaining callers and deletes the general API. The text below
describes the preceding first slice.

Implemented 2026-10-07. Successful Integer addition falls from **1,083.375 to
829.375 sends**, 23.4% fewer. Its ABI category falls from **281 to 88** sends.
Physical allocations remain twenty Id Objects per addition. Against the original
3,846.375-send checkpoint, the four simplification slices together remove 78.4%.

## The two ordinary Id messages

| Execution message | Native call and result conversion |
| --- | --- |
| `optionalWordResultOf:selector:argument:resultFactory:` | Send one Object argument to the target; Unit becomes language nil, Word is wrapped using the supplied factory's current `fromBits:` |
| `objectResultOf:selector:wordArgument:` | Obtain the argument's native Word using its current `bits` message, then send it to the target; Object result is returned, nullable Object becomes language nil |

All exposed arguments are Objects. Wrong arity/kind/null inputs produce native
`INVALID_REQUEST`; wrong native result kind is rejected. Target/getter/factory
failures retain their native status and detail. These interfaces describe fixed
conversion shapes; they do not guess types or reconstruct a dynamic signature.

Both functions scope receiver, target, selector, argument and any result factory
before dependency sends, and reuse **the existing `invoke_values` conversion core**
with fixed result/argument kinds. They introduce no second marshaler, compiler
option, value language, ABI field, plan Object or permission protocol. Actual target
calls, Word getters and result-factory constructors remain current Id messages;
Execution never reads another owner's payload or invokes a cached foreign entry.

## Authored consumers

`integerBitsOf:` uses the optional-Word form with an Object argument. The mixed
Float fallback uses the same form for `Float bitsOf:`. `integerFromBits:` uses the
Object-result form with a Word argument. Their Vector literals, signature arguments
and the now-unused `intOptionalBits`/`intObjectBits` constants are removed.
Signed arithmetic, overflow/division errors and mixed-number policy remain in Birnam.

The general signature-based bridge and its guarded Vector-elision optimization
remain for **current callers**, including multi-argument/byte/native-value paths.
This slice does not claim those APIs are retired or all fixed consumers migrated.
Remaining native-value/printing setter and byte cases belong to W04/W05.

This is a deliberate source/API simplification. The typed path does not reproduce
interception or failure points of the removed Vector/signature algorithm. It still
honors the actual target, getter, result-factory and typed-helper messages, including
replacements that allocate, collect, change another binding or fail.

## Send accounting

The previous three hot bridge calls contained 100 sends asking permission to omit
an argument Vector and 60 obtaining/checking signature bytes. Both disappear.
The actual native calls and conversions still cost 76 sends; typed bridge entries,
necessary selector interning and immediate result protection bring that bucket to 88.

| Per successful addition | Before | After |
| --- | ---: | ---: |
| Total sends, including amortized fixture work | 1,083.375 | 829.375 |
| ABI bridge category | 281 | 88 |
| Capture access | 181 | 174 |
| Validation/other binding lookup | 102 | 60 |
| Other lifetime/cleanup | 105.25 | 93.25 |
| Closure construction | 272 | 272 |
| Physical Id Objects allocated | 20 | 20 |

The 254-send saving includes 193 bridge sends, fewer captured signature bindings,
lookup of argument factories that are no longer needed, and less cleanup. These
categories are disjoint. The old optimized path already omitted physical argument
Vectors on this benchmark, so the gain is not an allocation-count reduction.

[Receipt](../measurements/fixed-abi-simplification-20261007.json) records exact
hashed traces and source/library identity. Inputs/libraries stayed unchanged during
the final census/timing; tracked root tokens end at zero. Separate two-second CPU
probes observed medians approximately 103 microseconds/addition before and 86 after,
about 17% lower. This is directional active-host evidence, not a paired confidence
interval, startup benchmark or universal performance guarantee.

Closure construction is now the largest category: 272 sends/addition (32.8%), followed
by capture access at 174 (21.0%). The corresponding systems still warrant analysis;
this measurement is not permission to delete all sends in either category.

## Validation

Passed:

- Native target and argument carrier/count checks; optional Unit versus numeric zero;
  full-width Word wrapping; Object/null normalization and wrong result-kind rejection.
- Precise GC during target calls and Word extraction, after external input roots are
  deliberately removed. Target and result-factory changes are visible at their actual
  send: a getter replaces the target; a target replaces the factory.
- Target/getter/typed-helper failures preserve status/detail; getter failure prevents
  the target call. A replaced typed helper is not bypassed.
- Typed calls still work when Vector permission and signature-byte query methods are
  replaced with failures: neither removed surface is consulted.
- Integer's independent signed-boundary/error oracle, division/remainder cases,
  Error-value identity, Float dispatch and collection/reload behavior.
- Existing general bridge/Vector elision, native-value ownership, incoming ABI carrier
  conversion, direct capture construction and captured Cells across wait/resume/GC,
  cancellation/cleanup at quanta 128 and 1.
- Execution Rust tests and workspace all-target compilation.

The new messages call the already tested adapter core with explicit constant kinds;
no new formal proof of the emitted runtime is claimed. Existing reachability rules
supply scoped-input lifetime premises; concrete collecting/replacing provider tests
above establish this slice's runtime evidence. No full image/startup benchmark,
full 171-check task rerun or subprocess-worker image acceptance is claimed.

The diagnostic prefix's early authored modules were explicitly compiled with the
retained Rust grammar seed, then production Bootstrap was restored and the seed
removed. Native code needed for a cold bootstrap remains. No installed prefix or
image is published by this turn; machine-code-bearing images are not implicitly
recompiled on startup. No installed prefix or image is published by this implementation turn.

Reproduce the census with a compatible rebuilt diagnostic prefix:

```sh
python3 scripts/microbench.py --case public-integer-bindings --counts-only --trace \
  --output /tmp/birnam-fixed-after/final-trace.json
python3 scripts/analyze-send-tree.py /tmp/birnam-fixed-after/final-trace.json \
  --output /tmp/birnam-fixed-after/final-analysis.json
```
