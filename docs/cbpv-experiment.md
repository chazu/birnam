# A CBPV Compiler Core for Birnam

**Status: experimental direction to try, not an adopted architecture.** 4 November 2026.

We will investigate call-by-push-value (CBPV) as a semantic foundation and intermediate representation for Birnam's authored compiler. The trial asks whether an explicit distinction between values and computations improves lowering, reasoning, and optimization while preserving the language already described in [ARCHITECTURE.md](../ARCHITECTURE.md).

The decision to try this does not select CBPV permanently, change Birnam's syntax or evaluation strategy, settle its dispatch alternatives, or authorize relaxing an invariant to make an experiment pass. The first milestone remains an Odin seed interpreter running Birnam-authored PEG. No CBPV runtime, compiler pass, or preservation result has yet been implemented in this checkout.

## Motivation and hypothesis

Birnam combines eager message sends, explicitly deferred Blocks, mutable object behavior, first-class Methods, and eventual native compilation. Its compiler needs to distinguish an already-obtained object reference from the work required to obtain or use one. That distinction becomes especially important when an argument evaluation changes a method definition, a custom lookup has side effects, or a captured Method must outlive replacement.

CBPV separates values from computations and provides explicit constructs for sequencing and deferred execution. Levy gives translations from call-by-value and call-by-name calculi into CBPV and studies their semantics. These results motivate the experiment; they do not prove correctness for Birnam's reflective, identity-bearing object system. [Levy, Call-By-Push-Value](https://pblevy.github.io/papers/hosc04.pdf)

The hypothesis is that a Birnam-specific CBPV core can make effect order and control flow easier to inspect and transform than an ad hoc execution representation, while still lowering ordinary execution to native calls and branches. A failure to demonstrate that advantage is a valid reason to use a simpler IR.

## Position in the system

The proposed pipeline is:

```text
Birnam source
    → authored Lang using authored PEG
    → authored compiler
    → experimental CBPV representation with Id operations
    → C and native code
```

Id continues to define object identity, behavior, lookup, and the selected Method's entry contract. CBPV describes how computations using that protocol are composed. Eventually Id's authored implementations could themselves compile through the same core. No additional semantic authority is implied by calling this a layer “beneath Id.”

The IR nodes, code objects, compiler passes, and any language-visible suspended-code objects remain ordinary Birnam objects with replaceable behavior. An internal distinction between semantic sorts is allowed; a fixed native catalog of Birnam values or a second authoritative object model is not. Odin remains bootstrap machinery. Its context and ABI knowledge stay within Odin code.

The experimental core must accommodate whichever dispatch contract is ultimately selected. It does not decide between plain Id, receiver-aware lookup, or independent bind/send policies. In particular, it cannot supply information to lookup that the selected interface does not receive. See [the dispatch review](dispatch-design-review.md).

## Values and computations in the trial

An existing Oop is a value. Copying that reference does not duplicate the referenced object. Allocating an object, reading mutable state, resolving a namespace binding, looking up behavior, and sending a message are initially treated as potentially effectful computations.

For explanatory purposes, `F Oop` denotes a computation producing an Oop. `U B` denotes a value containing a suspended computation of kind `B`. These are semantic distinctions, not source-language type annotations or requirements for a second runtime representation.

A Method or Block object is an identity-bearing value representing code and captures. Executing its selected implementation is a computation. Creating the language-visible Method or Block is also a computation, because construction can establish identity and have observable effects. A textbook pure thunk is therefore not automatically interchangeable with a Birnam Block allocation.

The trial can use internal administrative thunks where their creation, identity, and lifetime are unobservable. Eliminating those is a different claim from eliminating a source-visible closure. Any optimization must state which case it handles.

## Initial operation vocabulary

The following is a semantic sketch, not a finalized object API, binary encoding, or native calling convention:

| Form | Intended meaning |
|---|---|
| `return v` | Finish a computation with an existing value. |
| `let x <- c in k` | Execute `c`, then continue with its result bound to `x`. |
| `idSend(r,s,args)` | Perform the currently supported Id send operation using already-evaluated Oops. |
| `enterSelected(m,r,args)` | Enter an already-selected Method directly, without another implicit invocation lookup. |
| `makeBlock(code,environment)` | Construct an identity-bearing Block through the current supported construction behavior. |
| `resolveBinding(namespace,name)` | Resolve a name using the current namespace/environment behavior. |

The compiler may represent operations through object messages rather than these spellings. The operation vocabulary does not grant permission to hardwire replaceable policy. For example, `makeBlock` cannot permanently bypass a replaced constructor, and `resolveBinding` cannot read a private native namespace table instead of honoring the defined behavior.

`let x <- c` is computation sequencing. It is not Id's method-binding operation. The argument list shown in the sketch also does not choose a Vector ABI or require allocation of a new argument object for every send.

The initial representation keeps `idSend` as an operation with the finalized dispatch contract. Decomposing it into selection and entry is permitted only with a stated contract for effects and publication between those stages. The experiment must not accidentally adopt the extra dispatch layers from an unresolved proposal.

## Example lowering

Consider:

```birnam
[[factory receiver] run: [installer replaceRunAndAnswer]]
```

Its schematic lowering is:

```text
let receiver <- idSend(factory, #receiver, []) in
let argument <- idSend(installer, #replaceRunAndAnswer, []) in
idSend(receiver, #run:, [argument])
```

The receiver expression executes first. The argument expression executes next and may install a replacement `run:` Method. The final send must observe the behavior applicable after those effects. Hoisting target selection before the argument is not justified simply because the receiver value is already known.

Blocks remain explicit values whose invocation is a message:

```birnam
let original := [| 42];
let wrapper := [| [original value]];
[wrapper value]
```

The source creates two Block objects according to the current construction semantics. The outer `value` send uses the wrapper's current behavior; the body's `value` send uses the original Block's current behavior. Replacing either relevant method, inspecting identity, or observing construction may distinguish this program from one using only `original`.

Consequently, textbook thunk/force equations may simplify compiler-internal terms only when their assumptions hold. They do not justify replacing a source `value` send with an unconditional execution of captured code, merging distinguishable Blocks, or replacing a selected Method with a live name that rebinds on entry.

## Invariant audit

| Invariant | Failure to avoid | Required trial behavior |
|---|---|---|
| One object universe | Native CBPV values or contexts become a hidden semantic system | Language-visible values and compiler structures remain Birnam objects; Oops cross Id boundaries. |
| Payload opacity | Generic IR execution inspects Integer, Block, or collection layouts as a fixed type catalog | Object implementations interpret their own representations; specialized access requires valid assumptions. |
| Ordinary integer objects | Native immediates escape as a new semantic value representation | Unboxed temporaries remain internal; observable identity and reconstruction stay correct. |
| Replaceable messages | `force`, arithmetic, branching, allocation, or lookup opcodes permanently bypass behavior | Preserve current messages or justify and invalidate a specialization. |
| Direct Method entry | Lowering introduces an implicit second `apply`/`invoke` send | Enter the actual selected Method directly. Returning an Oop does not execute it. |
| Identity and sharing | Extensional rewrites duplicate, merge, or erase distinguishable objects/captures | Compare identity and alias relationships as well as equal returned data. |
| Effect order | Hoisting lookup, duplicating a computation, or dropping constructor effects changes execution | Preserve the specified sequencing and all observable effects. |
| Live replacement | Effect assumptions or inlined code remain valid merely because a selector name is unchanged | Track the relevant implementation dependencies and recover before the next affected operation. |
| Captured Methods | A retained Method silently becomes a fresh lookup on every entry | Preserve the selected implementation; its deliberate reads of mutable state retain their normal meaning. |
| Error values | CBPV sequencing implicitly throws or short-circuits on every Error object | Encode Birnam's actual propagation, recovery, and source-unit abort behavior. |
| Runtime malleability | A privileged native effect handler permanently owns scheduler, collector, or I/O semantics | Runtime services remain replaceable objects; native operations realize their supported behavior. |
| Native interoperability | CBPV leaks Odin context or adds a foreign value hierarchy to the ABI | Preserve the C-compatible, Oop-only Id boundary and Alien contracts. |
| Lifetime | A computation or native frame hides the only live reference from collection | Preserve receiver, argument, capture, Method, and executable-code lifetime across calls and suspension. |
| Images | Semantic continuations are mistaken for a guarantee of native-stack persistence | Persist supported object/code state through object-defined protocols; active native stacks remain outside the guarantee. |
| Independent bootstrap | The experiment requires the previous Rust system or a preinstalled Birnam | Use the Odin seed and authored modules, keeping dependencies explicit. |

The audit finds no necessary contradiction in using CBPV as the compiler's core. Compatibility depends on these interpretations and obligations; the choice of calculus alone cannot enforce them.

## Effects and optimization policy

Initially, arbitrary message sends and lookup are treated conservatively. “Currently returns an immutable value” does not mean “pure”: reading mutable state, performing lookup, and constructing that value may have effects. Nor does a method named `size` or `value` acquire permanent compiler privileges.

An optimizer may prove narrower behavior for a particular implementation. The proof or assumption must include the relevant replacement dependencies. Replacing that implementation invalidates the assumption before it can cause an incorrect affected operation. Returning the same Method from a lookup cache is insufficient if omitted lookup executions would have logged, mutated, suspended, or produced other observable behavior.

First candidate transformations should be administrative: eliminate redundant sequencing of an existing value, simplify compiler-internal continuations, and remove internal thunk/force pairs whose identities and construction are unobservable. Branch specialization, closure elimination, and unboxing require stronger evidence, especially because source control flow and construction are messages.

CBPV has compiler-oriented normalization and join-point research, including mechanized preservation results. Such work suggests useful techniques for exposing control flow without duplicating code. Its theorems do not automatically cover Birnam's observable allocation, reflective Methods, or concurrent publication. Each imported transformation must be checked against our richer observation model. [Commuting Conversions and Join Points for CBPV](https://doi.org/10.1145/3798210)

No initial purity annotation, effect row, or ownership discipline is added to the Birnam language. An internal analysis may use such information if it preserves arbitrary user-defined objects and replaceability. Its output is evidence for a transformation, not a new semantic authority.

## Errors and control effects

An Error Oop can be an ordinary result of `return`. Sequencing alone must not convert it into an exception. Birnam's existing error propagation and recovery are modeled explicitly, including the rule that a discarded Error aborts a source unit at that form while earlier effects remain. A successful result and a returned Error cannot be distinguished by an undocumented Odin result union at the Id boundary.

CBPV can model control and other effects, but algebraic handlers, first-class continuations, multi-shot resumption, and lazy evaluation are not automatically part of this experiment. They would require separate language and resource-lifetime decisions. [Levy, Effects and CBPV](https://pblevy.github.io/mgs/effcbpv.html)

The experiment does not reinstate stacklessness. A semantic continuation is a description of remaining computation; it need not be a heap object at every native call. The backend may use ordinary native calls/stacks where appropriate. Compiler-inserted safepoints, fiber suspension, and Alien callback transitions must still obey their separate contracts. A normalization pass must not erase necessary scheduling/collection opportunities.

Deoptimization must resume remaining work without redoing completed effects. An explicit sequencing representation may help identify that boundary, but reconstruction metadata and native recovery remain implementation work. CBPV does not make arbitrary C frames reifiable or make foreign calls safely repeatable.

## A bounded trial

The trial fits after the seed can execute enough source to express the relevant operations. It should reuse that parser and object floor rather than create a competing bootstrap language. PEG-on-seed remains independently deliverable even if the CBPV trial is rejected.

### Stage A Define the semantic subset

Represent existing values, sequencing, ordinary sends, selected Method entry, source Block construction, and captures. Specify the effects and observations of each operation. Use the seed's existing supported syntax and failure behavior. Unsupported forms must be reported explicitly; no silent fallback may make an experimental test appear to pass.

### Stage B Compare a direct evaluator with a CBPV evaluator

Implement lowering and the experimental IR/evaluator as authored Birnam objects when the seed supports them. Compare the same source programs through the direct seed evaluator and the lowered path. Share the Id implementation to isolate lowering differences, but use independent expected traces and mutation tests so a shared bug cannot pass solely through agreement.

Record returned values, required identity/alias relationships, Error kinds, ordered effects, binding/entry observations relevant to the chosen dispatch contract, and behavior after replacement. Compare graph relationships within each run, not raw pointer addresses between separate processes. Arbitrary object identity and closure equality require deliberate observations; printing equal results is insufficient.

### Stage C Lower a small subset to C

Once Stage B passes, emit C for the same bounded subset using the reviewed native boundary. Demonstrate that ordinary sequencing and selected Method entry can become native control flow without a compulsory interpreter hop or a new allocation for every semantic computation. Compare its behavior with the earlier paths. This is not yet a full optimizing compiler or a self-hosting claim.

### Stage D Try one justified transformation

Choose an administrative transformation whose source and target observations are stated precisely. Validate it with identity, effect, and replacement counterexamples as well as ordinary examples. Measure any code-size or execution benefit. More passes are justified only after this first transformation demonstrates the usefulness of the representation.

The trial introduces no fibers, general effect handlers, image format, or speculative optimizer solely to validate CBPV. Those remain later integration obligations if the core survives evaluation.

## Acceptance witnesses

| Witness | Expected property |
|---|---|
| Receiver and arguments each append distinct markers | They execute once, in the language-defined order. |
| An argument replaces the pending target Method | The eventual send observes the applicable replacement. |
| A source Block's `value` behavior is replaced | Invocation follows the current behavior; raw forcing does not bypass it. |
| Lookup returns a Method with an `apply`/`invoke` method installed on it | Ordinary sending enters the selected Method without consulting that extra method. |
| Two references to one Block versus source-distinct Block objects | Required sharing and distinctions survive lowering; equal results alone are not sufficient. |
| An observable construction operation is replaced | Lowering preserves its effects and respects replacement. |
| Lookup logs or mutates on each binding | Warm execution does not omit required lookup effects. |
| Capture Method M, replace its dictionary entry, then enter M and perform a fresh send | Captured entry and subsequent sending retain their distinct agreed behavior. |
| A selected Method returns a Block | The returned Block remains a value until explicitly invoked. |
| A returned Error is inspected/recovered, and a separate discarded Error ends a source unit | Both error paths match the existing language; no blanket exception conversion occurs. |
| GC occurs during a nested call with live captures | Captures, receiver, arguments, selected code, and results remain valid. |

Later native/fiber acceptance must add suspension between selection and entry, concurrent publication, Alien callbacks and retained pointers, and code retirement. Passing the sequential trial is not evidence for those properties. Image tests must eventually preserve captured Method/closure identity and data without promising active-stack serialization.

## Evidence and decision criteria

Keep three results separate: semantic agreement for the tested subset, usefulness of the IR for transformations, and native performance. A more legible IR may be worthwhile before it yields a speedup, but it should not be described as a performance improvement on that basis.

Measure lowering cost, native code size, allocations, retained memory, and execution cost against the direct path on the same workloads and collector configuration. Include ordinary stable sends, a reused Block, a source-created closure, effectful lookup, and replacement. Report cold versus warm behavior separately. Inspect generated C to detect universal wrappers, unnecessary argument packing, or accidental hidden dispatch layers.

Continue exploring if the core preserves the invariant witnesses, expresses the required operations without special-case bypasses, and provides a concrete improvement in reasoning or transformation structure at acceptable implementation cost. Adoption as the main IR requires an explicit later decision supported by the trial and a plan for the remaining concurrency/native obligations.

Revise or reject the direction if it needs a second native object system, bypasses replaceable behavior, depends on extensional equalities that discard required identity, makes every computation pay avoidable runtime packaging costs, or expands the bootstrap substantially without a demonstrated benefit. Do not weaken the architecture or relabel a failed witness to retain the experiment.

If rejected, keep the agreed source language, Id contracts, authored PEG/Lang/compiler trajectory, and useful acceptance witnesses. Remove the dependency on the experimental core from the compiler path. The seed/direct evaluator remains the reference for the supported subset.

## Questions left open by the experiment

The exact IR object protocol, internal typing discipline, treatment of administrative thunks, and relation to a later control-flow representation remain to design. So do the final dispatch choice, argument ABI, concurrent publication points, and the proof obligations for speculative optimization. A full formal preservation theorem would need an explicit Birnam source semantics with identity, mutation, lookup effects, Errors, and the desired reflective observations.

The intended outcome of this work is evidence about whether CBPV helps implement Birnam faithfully. “We are going to try it” is the current decision; “CBPV is Birnam's permanent execution foundation” is not.
