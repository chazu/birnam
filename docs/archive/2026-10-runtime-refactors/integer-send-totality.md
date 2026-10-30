# Where Integer addition's sends come from

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

Measurement-tool note (2026-10-28): the Operand/Outcome profiling runner and its
disconnected dispatch counters are retired. Commands below using that runner are
historical; use each report's measured revision to reproduce its results. Current
Oop checks include `scripts/oop-id-slice-bench.py` and the
`integer_performance` test. This note does not change the reported measurements.

Historical pre-simplification measurement, 2026-10-07. The
[implemented construction simplification](closure-construction-simplification.md)
reduces the total to 1,728.375 sends/addition.

The dominant system is **Block construction and captured-variable
access: 2,791 sends per addition, 72.6% of the total**. The native arithmetic and
overflow checks account for 88 sends, 2.3%.

## Measurement and accounting

The warmed `public-integer-bindings` candidate executed sixteen successful additions
and recorded every actual Id send with its parent activation, selector, opaque
receiver/result identities and inclusive subtree size. There were **61,542 sends**:
3,846.375 per addition including the fixture. Every `+` subtree contained exactly
3,845 sends; the remaining 22 were fixture overhead.

This is the current dirty worktree, including the unfinished invocation-input root
changes. It is not the earlier approximately 3,890-send revision. Inputs did not
change during the run; the benchmark checked the result and ended with zero tracked
root tokens. This is a message census, not a CPU profile or a theoretical lower bound.

The table charges nested lookup, admission and lifetime work to the operation that
requested it. Categories are disjoint and account for every recorded send.

| Cause | Sends/addition | Share |
| --- | ---: | ---: |
| Closure construction, capture storage and preparation | 2,241 | 58.3% |
| Capture retrieval and its preparation | 550 | 14.3% |
| Boolean selection primitives | 300 | 7.8% |
| ABI bridges and immediate returned-object retention | 281 | 7.3% |
| Other lifetime protection and cleanup | 243.25 | 6.3% |
| Validation and other binding lookup | 102 | 2.7% |
| Word arithmetic and guards | 88 | 2.3% |
| Dispatch/policy scaffolding | 25.125 | 0.7% |
| Other name handling | 16 | 0.4% |

Rounded percentages need not sum to exactly 100%. Inclusive application envelopes
such as `integerBinary:operation:` and `ifTrue:ifFalse:` overlap these categories;
their totals must not be added to this table.

### Why a selector histogram obscured the cause

The largest individual selectors are `retain:` (630.0625/addition), `intern:` and
`symbolNamed:` (524 each), `release:` (378.0625), and `identityOf:` (273).
But closure construction alone causes 308 of those retains, 198 releases, 374 of
each name operation and 259 identity queries. Removing lifetime work in isolation
misses the machinery that creates the need for most of it. The earlier root-first
recommendation over-weighted the leaf histogram; the full tree changes the priority.

## Why eight Blocks are made for one addition

[Integer policy](../../../modules/integer.birnam) has four conditionals on this path:
valid left operand, Error operand, Integer versus Float operand, and overflow.
Each sends `ifTrue:ifFalse:` with two Block arguments. The arguments are created
before the conditional runs; [Boolean policy](../../../modules/boolean.birnam) selects
and invokes only one. The successful path therefore **constructs eight Blocks and
invokes four**, writes 39 capture slots, and performs 69 capture projections.

| Branch Block | Captures | Construction sends | Invoked in this run? |
| --- | ---: | ---: | --- |
| Invalid-left result | 0 | 168 | No |
| Continue with valid left | 11 | 421 | Yes |
| Return incoming Error | 1 | 191 | No |
| Continue with non-Error | 11 | 421 | Yes |
| Integer arithmetic | 4 | 260 | Yes |
| Float/type fallback | 9 | 375 | No |
| Construct successful result | 2 | 214 | Yes |
| Overflow result | 1 | 191 | No |
| **Total** | **39** | **2,241** | **4 of 8** |

The four unused Blocks cost **925 construction sends/addition**, or **24.0% of all
sends**. Their bodies do not run. This is a measured opportunity, not proof that
all those sends can be omitted under arbitrary method replacement or reflection.

## What closure construction actually does

The [synchronous emitter](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-native-backend/src/native_sync.rs)
lowers each Block literal into reference storage allocation, one ObjectModel write
per capture, and a Block-owner constructor message. These disjoint subtrees and
adjacent preparation account for the entire 2,241-send category:

| Construction step | Sends/addition |
| --- | ---: |
| `fromMethod:codeObject:referenceSlots:` and descendants | 1,153 |
| `allocateReferenceSlots:` and descendants | 368 |
| `slotAt:in:put:` and descendants | 390 |
| Adjacent ObjectModel/Block factory lookups | 220 |
| Immediate roots of returned objects | 110 |

The [Block constructor](https://github.com/chazu/birnam/blob/ec640c21aee5a245cebeb993ef7b7501decead3e/rust/birnam-execution/src/machine.rs) requests
`canonicalBytesOf:` from the current CodeObject owner, obtains Bytes through its
owner, and parses the portable program shape to validate arity/capture count.
It validates capture storage layout, reads its slots through ObjectModel, and
constructs the Block's delegated behavior and storage. The owner messages are
valid Id operations; their repeated use is a representation/protocol choice.

There are **eight canonical program exports per addition**, costing **568 nested
sends** (14.8% of the total, already included in construction). Re-exporting a
whole program merely to obtain its shape is a concrete simplification candidate.
A compact owner-provided shape operation could retain current dispatch while
avoiding serialization, Bytes construction and parsing on each branch creation.

## Why retrieving captures costs another 550 sends

At generated method entry, each capture is obtained through the current Block
owner using `captureWord:of:`. The emitter separately looks up the Block factory
for each capture and roots returned objects. The 69 projections account for:

- 69 each of `at:`, `intern:`, `symbolNamed:`, `symbolBytesOf:`,
  `captureWord:of:` and `prepareAllocation`;
- 136 `retain:` sends.

That is exactly 550 sends. `prepareAllocation` is called before the shared
Execution operation dispatcher chooses its operation, so even capture projection
passes through allocation admission. Whether that admission has a necessary
collection/mutation contract on this path needs a focused examination.

## Necessity and next investigations

The follow-up [closure necessity audit](closure-machinery-necessity.md) explains
each check and gives a disjoint purpose breakdown of all 2,791 closure-related
sends. It distinguishes genuine memory/lifetime protection from repeated
admission, temporary transport and immutable-code validation.

Necessary behavior includes choosing the correct branch; preserving captures and
meaningful Block identity/aliases when exposed; current Id lookup/replacement;
real effects and failure order; supported logical Frame/recovery state; and
object/code/buffer lifetime at callbacks and collection. None establishes a
requirement for these exact temporary allocations, repeated whole-program exports,
per-slot admission checks, or individually rooted capture projections.

Prioritize the closure/capture system (W07/W08/W12 in the
[simplification plan](runtime-and-abi-simplification.md)):

1. **Compact code shape:** determine what the constructor actually needs from
   CodeObject and replace full serialization with the smallest current-owner
   message. Do not cache a foreign executable or inspect foreign payloads.
2. **Capture protocol:** investigate one construction/access operation for a
   capture set, narrower admission, and minimal free-variable sets. Count all
   replacement lifetime and validation work; batching alone may hide complexity.
3. **Branch representation:** investigate virtual Blocks that materialize when
   observable, or a simpler conditional/closure representation. The 925-send unused
   portion identifies the experiment; it is not an unconditional deletion budget.

Lean should establish equivalence of logical captures/results and needed-value
reachability for one concrete transformation. TLA+ should search replacement,
callback/GC, escape/exposure and cleanup interleavings, with deliberately broken
variants producing counterexamples. Small runtime probes must exercise replacement
of conditional/factory operations, first-class Block use and failure paths.
The proofs must concern the proposed simpler protocol, rather than freezing the
current allocation/token history. No further optimization is implemented by this
analysis; the earlier input-root work remains in the worktree.

## Reproduction and evidence

```sh
python3 scripts/microbench.py --case public-integer-bindings --counts-only --trace \
  --output /tmp/birnam-send-totality/trace.json
python3 scripts/analyze-send-tree.py /tmp/birnam-send-totality/trace.json \
  --output /tmp/birnam-send-totality/analysis.json
```

[Compact receipt](../../measurements/integer-send-totality-20261007.json) records source,
runner/library and trace hashes, category selectors, application envelopes and
construction groups. Large raw traces remain local; the commands reproduce them.
Classification uses selector/argument labels and adjacency, checked against the
emitter and runtime paths; it is not an automatic proof of an owner's semantics.

The analyzer verifies exact agreement with the independent selector census, parent
ordering, every inclusive subtree size, full partition coverage and construction
group coverage. The dispatch-metrics regression passed and
`cargo check --workspace --all-targets --offline --locked` passed. Diagnostic
instrumentation is feature-gated; it reads Id-owned headers/Symbol names, supplied
ABI Bytes and opaque identities, adds no diagnostic sends, and changes no dispatch.
