# 74V — Virtual numeric intermediates with original Frame recovery

Status: compiler recovery maps implemented in an isolated development checkout;
runtime integration and all production/performance acceptance remain pending.
This continues the unboxed-arithmetic requirement in
[74T](74t-representation-and-allocation-optimizations.md), whose enabled numeric
fusion still allocates every original result box.

## Required behavior

Carry successful numeric intermediate results as raw bits until an observation
requires their original object identities. Derive operations from current
admitted native implementations, preserving exact overflow/error behavior and
ordinary Id malleability. Keep public ABI, canonical descriptors, V2 transitions
and image encoding unchanged. Equal numeric values from distinct allocations
remain distinct identities; repeated aliases of one result recover one object.

The initial supported bodies are those of the existing numeric Template:
bounded straight-line integer chains with entry values, local binds, literals,
Call/Continue edges and a final Return or TailCall. Unsupported operands,
errors, mutation, native declarations, storage or allocation budgets must take
the established path. Selector names do not confer numeric authority.

## Compiler maps

`numeric_chain::RecoveryMaps` is derived separately from the existing Template,
so the boxed consumer does not pay its preparation or metadata cost. Derivation
requires the numeric Template and exact canonical coloring. Every PC records
all physical aliases of prior numeric results at original action publication,
including the send operand window, and after its original continuation.

An alias names the numeric operation's result identity, never its numeric bits.
Absent aliases leave ordinary Oops in place, including input values and distinct
literal boxes. Local binds propagate result identities. V2 parallel copies read
their sources before destinations change; clears and call-result initialization
produce the successor map. The current pending operation has no result identity
at publication. A final TailCall creates a returned result but no local successor.

Initial tests cover shared local/send aliases, equal computations producing
distinct identities, final Return and TailCall, two-/three-/16-/32-operation
chains, slot reuse/dead-value clearing, and rejection of changed layouts and
observing operations. These are compiler checks, not runtime allocation savings.

The compiler checkpoint passes all 85 `birnam-codegen` debug tests and all five
recovery-map release tests. The added Continue case propagates one result
identity through 110 aliases across a canonical split without inventing another
numeric result. Verification commands, with an explicit isolated Cargo target:

```sh
cargo test --locked --offline -p birnam-codegen
cargo test --locked --offline --release -p birnam-codegen numeric_chain::recovery_maps
```

This checkpoint has not been integrated into the main checkout or through its
runtime consumer; the full source/production gates below remain required.

## Runtime integration still required

1. Extend private allocation-journal preparation to exact raw numeric payloads.
   Establish current representation and native constructor authority explicitly;
   do not disguise raw numeric bits as Oop payloads. Keep the original allocation
   charge, fresh identity and collector/finalizer behavior. Preparation is not an
   Id allocation; materialization must not charge again.
2. Use the existing raw numeric ticket and current binding/context guards to
   admit a bounded interval before any effect. Include all baseline allocation
   charges and journal capacity in admission. No observing callback, collection
   or native retirement may intervene while a live result remains virtual.
3. Execute original canonical PC entries and V2 transitions with private
   placeholders for admitted virtual results. Those entries only move/stage
   values in the supported Template; the raw ticket supplies numeric results.
   Existing literal/index/PC-token boxes remain ordinary allocations initially.
4. At every supported forced exit, materialize all published aliases of already
   charged results before ordinary dispatch or observation. Restore pending
   receiver/argument identities as well as local Frame aliases. Future
   predictions are not executed results and must not allocate or materialize.
5. Materialize results required at the final observation boundary, preserving
   return identity and any still-observable Frame aliases. Prove dead values
   before leaving them journaled. Do not assume that returning a value by itself
   proves the Frame is already retired.

## Completion evidence

Require real-source baseline/enabled/forced comparisons at every supported exit,
with actual physical allocation savings and unchanged logical allocation charges.
Exercise equal/distinct results, repeated aliases, captured inputs, original
numeric literals, extremal arithmetic, zero division, mixed/error refusal,
changed operators/API/layout, collecting observation, V2 capture/suspension,
native eviction/retirement and journal/automatic-collection limits.

Run debug/release correctness and the production source/install/image/archive/
relocation gates. Commit correctness before paired execution and collection
timings. Record preparation, metadata, native code and retained resources, plus
refusal and cold/application costs. Keep qualification gates unchanged. This
work does not complete the loop, aggregate or closure families, nor establish
the full arithmetic family until runtime behavior and its acceptance are proved.
