# Source compiler over Id

Codegen consumes Lang CST messages, retains the existing pure lowering and slot
coloring, and emits typed native entries using live Id messages. `compile:` returns
an owned resumable Method, `eval:` runs it on a real Machine, and `emit:` returns
the generated C String. TCC compiles and owns each Unit; its initializer consumes
Namespace and the Unit owner through the existing native module ABI.

Reflect `defineMethod:on:as:` compiles the retained named-adapter algorithm. Its
Method keeps both the code Unit and the implementation Block. Calls perform fresh
`value:` lookup on that implementation; replacement does not rewrite a captured
Method or a suspended frame. No generated Runtime service catalog is imported.

`deferred.rs` preserves the remaining Image/Store publication, inventories,
portable materialization and acceptance cases by function. It is not loaded or
compiled and does not supply compatibility services. Its replaced source facade
and bootstrap were removed. Allocation-elision remains separately retained in
Runtime and the pure compiler planners. Restoring those branches requires their
real owners and acceptance tests; a source-compiler pass does not validate them.

## Source and generated-code contract

`compile:`, `eval:` and `emit:` take source String Objects. Parsing uses current
Lang `parse:`; CST projection uses `ruleName`, `start`, `end`, `childCount` and
`childAt:`. The pure compiler retains its portable encoding, logical lowering,
slot coloring and canonical descriptor construction. The production native backend
emits the Id calling convention directly; it does not wrap exported legacy
all-Oop runtime functions. Baseline call sites publish the established Machine action and perform current
lookup when driven. The narrow discarded-clone consumer below can omit an admitted
callee body through the existing inline-call path.

Generated module initialization constructs one registered Method per implementation
using canonical descriptor and selection Bytes. Blocks copy captures through
`Block fromMethod:captures:` and Vector messages. Named adapters additionally use
`Method fromStep:descriptor:selection:owner:implementation:` so their implementation
Block and code Unit have distinct, explicit ownership. Top-level bindings publish
through Namespace `at:put:`; nested bindings remain lexical. Values and errors use
their current owners, with temporary generated values retained across callbacks.

## Generated execution

Each implementation has one ordinary resumable entry with a PC switch. Baseline
owner operations remain live Id sends. Automatic per-PC candidates, their Method/Vector
construction and their size-limit policy have been removed: they retained the same
work and added construction and guard costs without an established benefit.
There is no replacement background compiler or candidate service.

Explicit candidates and guarded recovery remain available in Execution for actual
optimizations. Broader region fusion, numeric reuse and allocation-elision need
concrete producers and evidence under [Design 90's reassessment](../../docs/design/90-optimizer-reassessment.md).

The production tests cover literals, sends, threaded messages, conditional and
nested capturing Blocks, named installation/replacement, failed compilation and
initialization, collecting factories, captured Methods and suspended frames.
Compiled protected-boundary exits deliver once into their callers; abandoned
waiting frames release their mappings. A separately authored C candidate
compiled by TCC recovers into a Codegen baseline, then removes its own binding and
collects before returning. This proves the compiled recovery boundary and mapping
lifetime; generated allocation-elision/deoptimization emitters remain separate.
Repeated compiled execution preserves effects of replaced Integer factories,
current numeric lookup and collecting Integer/Machine operations. Compilation,
larger functions and captured Blocks work even when speculative native Method
construction is rejected. Actual Unit counts and relocated bytes return to zero
after the final reference and checkpoint. Explicit candidate withdrawal/lifetime
and compiled recovery remain covered by their separate production fixtures.

The full Artifact/PortableCode language adapters, durable publication, embedding,
Image and CLI workflows are not accepted by these source compiler tests.
Their retained migration inputs are not loaded by Bootstrap. Current commands and
limits are in [validation](../../docs/validation.md).

Store/db and Cells now load through Bootstrap before the compiler. Their separate
[production suite](../birnam-store/README.md#validation) also exercises compiled
source consumers and Store-backed Globals. The current compiler still publishes
globals through Namespace; reconciling that caller remains upper integration work.
Store MOP hooks now use receiver messages and owned Execution Methods. Staged
compiler publication/compensation callers in `deferred.rs` still need migration;
the old Store native tables are gone.


## First allocation-elision producer

The native backend recognizes exactly two PCs: entry receiver/argument operations,
a zero-argument send to an entry argument, then an existing-argument return. No
captures, lexical bindings, extra operations or named adapter are admitted. It
adds `Method withDiscardedClone:answer:selector:return:in:` metadata to the ordinary
baseline, retaining the result in the same TCC Unit. No extra native entry or
per-PC candidate is emitted. The runtime checks the actual selected clone and
allocator owners; selector spelling is not a proof. An unrecognized or changed
operation takes the baseline, and bounded execution always does so.

[Production evidence](../../docs/design/90-discarded-clone.md) covers actual heap
inventory, replacement effects, alias recognition, escaping/bound values, paused
Frames, collection during guards and release of every compiled Unit. This is a
single narrow consumer, not restoration of the retained legacy elision planners.


## Combined arithmetic predicate and unboxed intermediate

The native backend recognizes a three-PC uncaptured callee: one binary send using
entry arguments, another binary send consuming that result and an entry argument,
then an explicit return (optionally through its final binding). It attaches
`Method withNumericPredicate:then:left:right:limit:return:in:` metadata, retaining
the ordinary baseline and same TCC Unit. Runtime Integer ownership determines
whether the sends are arithmetic and comparison; aliases are eligible. Captures,
extra operations, intermediate bindings and bare tail-call forms are not emitted
as candidates. Bounded calls and unsupported operations remain baseline execution.

The admitted predicate returns an existing Boolean while avoiding its intermediate
Integer and callee index boxes. No new C kernel, deferred identities or loop cache
is emitted. [The production receipt](../../docs/design/90-numeric-predicate.md)
records current semantics, allocation savings and the local debug comparison.
