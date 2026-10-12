# JSON numeric conversion — 2026-10-12

JSON lowering previously searched every numeric token separately for `.`, `e`
and `E`, then chose `asInteger` or `asNumber`. The retained rewrite removes those
searches from short tokens and stops the long-token search as soon as the kind
is known. This is ordinary Birnam code using current sends, with no additional
native primitive, method-replacement guard, compiler change or GC policy change.

## Algorithm and contract

A valid numeric token shorter than 19 bytes has at most 18 decimal digits if it
is an integer. Its magnitude is below 10^18, which is below 2^63. Therefore the
existing `asNumber` cannot take its integer-overflow-to-Float path on that input:
it returns the same numeric kind and value as the original classification.
Fractional/exponent forms already use `asNumber` and continue to do so.

For tokens of 19 bytes or more, the decoder checks `.`, then `e`, then `E`, stopping
at the first match. If none matches, it still calls `asInteger`. In particular,
`9223372036854775808` still yields nil rather than silently becoming a Float.
Signed limits, negative zero and the existing acceptance of leading zeroes remain
unchanged. Invalid numeric syntax is still rejected by the grammar before lowering.

The bound is a property of valid numeric text, not a speculative assumption about
method bindings. `size`, comparison, scanning and conversion remain ordinary
late-bound sends. Custom operations can change the new algorithm's behavior,
as they can in other authored Birnam code. The proof describes canonical operation
contracts; it does not equate internal send traces before and after a rewrite.

The JSON grammar and CST shape are unchanged. Existing cached grammars and
previously parsed nodes remain usable after source reload.

## Alternatives investigated

- **Integer-first parsing:** improved the integer-heavy application, but a
  float-only conversion benchmark became about 10% slower because every float
  first underwent an unsuccessful integer parse.
- **A byte probe instead of `size`:** worked, but did not outperform the clearer
  size comparison; an in-range probe also constructed a temporary result.
- **Length bound with the old eager long-token classification:** helped short
  numbers but added overhead to long float tokens. Short-circuiting the long
  classification improved the measured mixed long-float workload.
- **Grammar markers for fractional/exponent syntax:** promising CPU results,
  but changing number-node structure broke reload compatibility. A reproduced
  case parsed `1.5`, loaded the prototype, then got nil through both the cached
  grammar and the retained old number node. This representation change was not
  retained. Fixing it would require a separate compatibility design.

## Validation

The JSON suite now has 107 assertions covering numeric length boundaries, signed
64-bit extrema, overflowing integers, negative zero, short/long decimals, both
exponent spellings, malformed input and current conversion-method replacement.
The complete module suite passes 642 assertions. Application acceptance includes
50/500-record runs and edit–rebuild–run, and the phase harness checks independent
results. Reload tests retain an old grammar and Float node across the source
change. Mapped/rebuilt saved-image checks cover numeric kinds, extrema, overflow
and negative zero with the entry source removed.

Lean proves the decimal-digit bound, signed representability, the short-token
conversion equivalence under canonical parser contracts, and the long-token
classification equivalence. Mutations that admit 19-digit integers into the fast
branch or ignore uppercase exponents must fail. The formal gate passes 188
theorems, rejects 37 proof mutations, and checks all 118 TLA+ configurations,
including required counterexamples. This is not verification of the native
numeric parser or the entire compiler.

## Measurement

The paired benchmark compares the preceding committed JSON source with this
rewrite on the same runtime. It checks Python oracles and uses ABBA process order
with three warmed thread-CPU measurements per process. Individual workloads are
not additive slices. It separately covers integers, short/long float conversion,
long integers, and end-to-end parsing of Float arrays, so conversion wins cannot
hide parser costs.

```sh
git show dcd41253:modules/json.birnam > /tmp/json-before.birnam
python3 scripts/json-unescape-bench.py \
  --prefix /path/to/compatible/birnam-prefix \
  --baseline-json /tmp/json-before.birnam \
  --work /tmp/json-number-comparison \
  --phases full,numbers,float_numbers,long_float_numbers,long_integer_numbers,parse_floats,parse_long_floats
```

Normal timing runs retain the fixed 16 MiB GC budget. Allocation counters are
measured separately with automatic GC disabled. No user installation or release
was made.

## Results

The [accepted evidence](../measurements/json-number-conversion-20261012.json)
binds the measured sources and runtime to the following six-sample medians:

| Workload | Before | After | CPU change |
|---|---:|---:|---:|
| Full 50-record application | 349.8 ms | 330.3 ms | 5.6% less |
| Convert its 150 integers | 68.2 ms | 51.7 ms | 24.3% less |
| Convert 150 short floats | 64.7 ms | 48.0 ms | 25.8% less |
| Convert 150 long floats | 64.9 ms | 62.0 ms | 4.5% less |
| Convert 150 long integers | 64.7 ms | 74.2 ms | 14.6% more |
| Parse a 150-element short-float array | 84.7 ms | 68.3 ms | 19.4% less |
| Parse a 150-element long-float array | 84.5 ms | 81.8 ms | 3.2% less |

The long-integer regression is a real tradeoff. Those tokens pay the new length
check and then all long-path classification checks; they cannot safely use the
short-token conversion rule. This includes both representable 64-bit extrema
and overflow cases. The retained change improves this application and both
measured Float arrays, not every numeric workload. Individual long-float
spellings may differ from the mixed long-float result shown here.

The full pass adds 91,404 live objects and 2.43 MiB of allocation charge with
automatic collection disabled, versus 95,604 objects and 2.50 MiB before this
change. These are allocation diagnostics, not peak RSS. Source reload kept the
same cached grammar and correctly lowered a preexisting Float node; mapped and
rebuilt images retained signed limits, overflow and negative zero.
