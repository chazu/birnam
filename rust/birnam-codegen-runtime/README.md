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
all-Oop runtime functions. Every call site still publishes the established
Machine action and performs current lookup when driven.

Generated module initialization constructs one registered Method per implementation
using canonical descriptor and selection Bytes. Blocks copy captures through
`Block fromMethod:captures:` and Vector messages. Named adapters additionally use
`Method fromStep:descriptor:selection:owner:implementation:` so their implementation
Block and code Unit have distinct, explicit ownership. Top-level bindings publish
through Namespace `at:put:`; nested bindings remain lexical. Values and errors use
their current owners, with temporary generated values retained across callbacks.

The production tests cover literals, sends, threaded messages, conditional and
nested capturing Blocks, named installation/replacement, failed compilation and
initialization, collecting factories, captured Methods and suspended frames.
Compiled protected-boundary exits deliver once into their callers; abandoned
waiting frames release their mappings. A separately authored C candidate
compiled by TCC recovers into a Codegen baseline, then removes its own binding and
collects before returning. This proves the compiled recovery boundary and mapping
lifetime; generated allocation-elision/deoptimization emitters remain separate.

The full Artifact/PortableCode language adapters, durable publication, embedding,
Image, Store and CLI workflows are not accepted by these source compiler tests.
Their retained migration inputs are not loaded by Bootstrap. Current commands and
limits are in [validation](../../docs/validation.md).
