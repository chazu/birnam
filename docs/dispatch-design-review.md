# Lookup Binding and Sending in Birnam

**Discussion draft for programming language design review — 4 November 2026**

**Decision follow-up, 5 November 2026:** Birnam selected option B, receiver-aware lookup, for the seed. It may return an existing Method or a Method closure; mechanical send/bind enters the actual selection directly. The native calling ABI remains open. The discussion below is retained as the comparative review, including the alternatives and questions considered before that decision.

Birnam is a prototype-based, image-oriented language being rebuilt around Ian Piumarta's Id object model. The immediate design question is how much of message dispatch should be independently replaceable: lookup alone, lookup with access to the application receiver, or separate lookup, binding, and sending policies.

The central finding is that two questions must be separated. **What information can method selection inspect?** And **which dispatch stages are independently observable and replaceable?** Giving lookup the receiver closes an important gap in selection expressiveness. Adding separate bind and send hooks creates additional reflective boundaries, but those hooks are not necessary for many behaviors usually described as custom sending.

In particular, a lookup operation that can return a Method closure can implement a broad range of call policies. That gives a useful restricted equivalence at ordinary call boundaries. It does not establish unrestricted equivalence in a language where programs inspect Method identity, capture Methods, observe lookup effects, or modify the dispatch machinery while computations run.

The alternatives below are open for discussion. The earlier architecture required independently replaceable send and bind; simplifying that requirement is now under consideration. The argument is a mathematical design analysis, not a machine-checked proof of a complete language semantics. Readers need no familiarity with the rest of the Birnam repository.

## 1 The surrounding language commitments

Birnam's organizing principle is one malleable object universe. Values, Methods, code, compiler structures, namespaces, fibers, and runtime services are objects. Object interaction is through messages. Receivers, selectors, message arguments, and results are ordinary object pointers, called Oops. There is no second authoritative tagged-value system hidden in the implementation language.

Object payloads are opaque to generic Id. An Integer, for example, is an allocated object with its numeric contents in its payload; it is not a tagged immediate pointer value. An object's implementation interprets its representation. Generic dispatch knows the mechanical information required to reach behavior, but not the semantic layouts of every kind of object.

The bootstrap is being written in Odin, with a C-compatible boundary for generated code and native modules. Odin-specific context and calling conventions remain within Odin code. The intended compiler, Lang module, and PEG module are Birnam-authored. Initial compilation emits C; native speed is a goal to be demonstrated, not an implication of emitting C.

Several decisions constrain the dispatch discussion:

- Ordinary lookup selects a Method, and execution enters that Method directly. There is no second implicit lookup of `apply` or `invoke` on the selected Method.
- Creating a Method and installing it into a dictionary are distinct operations.
- Subsequent sends observe supported method replacement. Already-entered calls and explicitly captured Methods retain their selected implementation.
- If an optimized operation relies on an assumption that becomes invalid, the next affected operation must use valid behavior, deoptimizing if necessary, without replaying completed effects.
- Runtime policy is inspectable and replaceable. Arbitrary replacement need not preserve correctness or keep the process alive, but supported normal operations must behave coherently.

The runtime will eventually have stackful fibers on multiple OS workers, conservative collection, and initially non-moving allocation. Images preserve object graphs, code, and source through object-defined persistence. These decisions matter for Method lifetime, concurrent replacement, and the cost of policy closures, but they do not by themselves choose a dispatch protocol.

## 2 Vocabulary and the basic Id model

Use the following terms consistently:

| Term | Meaning here |
|---|---|
| Receiver | The application object to which a message is sent. |
| Selector | The interned name of an operation. |
| Vtable | An object describing how messages for a family of objects are resolved. |
| Lookup | An operation on a vtable that selects a callable Method for a selector. |
| Binding | The operation that obtains the selected Method for an application receiver and selector. |
| Sending | Binding followed by entering the selected Method with the receiver and evaluated arguments. |
| Method | A first-class callable object, possibly carrying captured data. |
| Entry | The mechanical operation that starts execution of an already-selected Method. |

“Method or Method closure” does not require two incompatible callable categories. A Method can have an empty environment or carry data. Both obey the same entry contract. Capturing a Method also does not necessarily capture its application receiver: in the model below, the receiver is supplied at entry unless the Method deliberately captures one.

In Piumarta and Warth's basic implementation, `send` and `bind` are native glue; replaceable `lookup` is installed on the vtable of vtables. Binding obtains the receiver's vtable and sends it `lookup`. Looking up lookup is itself reflective, with a native table-lookup base case for the self-referential root vtable. The closure extension lets dictionary entries carry code and arbitrary data, passed to the selected implementation at entry. [Open, Extensible Object Models, sections 3.2 and 4.1](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf)

For the rest of this discussion, write:

```text
r        application receiver
s        selector
a        already-evaluated arguments
v(r)     receiver's vtable
L0(v,s)  lookup using vtable and selector
E(m,r,a) direct entry of Method m

bind(r,s)   = L0(v(r),s)
send(r,s,a) = E(bind(r,s),r,a)
```

This is a behavioral model, not a proposed concrete C signature. State, errors, and effects are made explicit when needed below. The lookup result is whatever Method lookup actually selects; it need not be an entry physically stored in the original dictionary.

The displayed select-then-enter equations assume successful selection. If binding fails, the defined failure outcome must be returned or handled without entering it as a Method. Moving a failure from binding into a carrier Method's body may preserve an ordinary send's result while changing what an explicit bind observes; that is another instance of the observation-boundary problem.

## 3 The independent design axes

### Information available during selection

There is a material difference between selecting with `(vtable, selector)` and selecting with `(vtable, selector, receiver)`. Two receivers can share the former inputs while having different identities and state.

Access to evaluated arguments is a further extension. Neither plain lookup nor the receiver-aware bind proposed here receives the arguments. Either can return a Method that inspects them when entered. If an explicit binding operation must return different Method identities based on argument values, its interface must receive those arguments or some equivalent information.

Information supplied through a “current invocation” variable, stack inspection, or thread-local context also counts. Such mechanisms do not prove that a narrower interface is sufficient; they enrich it through another channel and introduce their own reentry and concurrency obligations.

### Independently replaceable stages

An independently replaceable stage can have its own identity, state, effects, installation point, and replacement lifetime. A separate send hook can be invoked for ordinary sends while explicit binding bypasses it. A separate bind hook can change method selection while the lookup implementation remains installed and independently inspectable.

Those distinctions can be useful even when the same input/output function could be expressed through a single hook. They are questions about reflective API structure, not simply about which calculations the language can express.

### What lookup is allowed to return

A lookup restricted to returning pre-existing dictionary entries is weaker than a lookup that may synthesize a Method closure. The latter can return executable policy that captures the selector, another Method, a receiver, or arbitrary objects, subject to what was available during selection.

That capability is central to the simpler alternatives. If it is permitted, it must be part of the explicit contract rather than an accidental implementation trick. Tools must then distinguish “the Method selected for this operation” from “a Method stored under this name in a particular dictionary.”

### What the observer can distinguish

Several notions of equivalence are possible:

1. The same final return value.
2. The same returned identity, errors, divergence, and ordered application effects.
3. The same explicit binding results and Method identities.
4. The same reflective view of lookup, hooks, stack frames, allocations, and captures.
5. The same observations under every concurrent publication and scheduling history.

An argument about level 2 must not be advertised as a proof of levels 3–5. Equal computability is weaker still: two Turing-complete designs can have substantially different APIs and program transformations.

## 4 The alternatives

### A Plain Id with expressive lookup results

Keep fixed mechanical send/bind and replaceable `L0(v,s)`. Lookup may return an existing Method or a Method closure. Entry calls it directly.

This has the smallest extension surface. Receiver-sensitive or argument-sensitive execution can live inside the returned Method. The limitation is that selection itself cannot distinguish receivers sharing a vtable unless some additional channel or representation distinction supplies that information.

This candidate is explicitly recorded for consideration. Choosing it would revise the previous requirement that send and bind must themselves be independently replaceable methods.

### B Receiver-aware lookup

Extend lookup with the application receiver:

```text
L1(v,s,r)    → Method
bind(r,s)   = L1(v(r),s,r)
send(r,s,a) = E(bind(r,s),r,a)
```

The send/bind skeleton remains fixed. Selection can inspect receiver state before returning a Method. It can choose a pre-existing Method or construct a closure capturing a decision made at that moment.

There are two receiver roles: lookup's own receiver is the vtable, while `r` is an explicit application receiver. The latter does not replace lookup's `self`. Recursive meta-lookup must keep those roles straight and define the corresponding native base case; changing an equation's parameter list alone does not complete that implementation.

The extra receiver parameter does not force lookup to inspect receiver state. Ordinary vtable-based lookup can ignore it. It does, however, mean that `(vtable, selector)` is no longer a generally sufficient result-cache key for arbitrary lookup implementations.

### C Independently replaceable receiver-aware bind

Expose a bind policy `B(r,s)` while retaining a lookup operation it may use. For example, bind may select a backend Method directly, delegate to normal lookup, or perform an effect and then look up a Method.

At the level of an opaque selection function, B and C have the same relevant inputs: `L1(v,s,r)` can run `B(r,s)`, and `B(r,s)` can run `L1(v(r),s,r)`. This assumes that obtaining `v(r)` has no additional semantic effect and both policies have the same ordinary language capabilities.

That correspondence does not preserve every reflective observation of separately installed hooks. C makes bind and lookup separately addressable components; B combines method selection into lookup. Whether that distinction is desirable is a separate question from access to the receiver.

### D Independently replaceable send and bind

Add a send policy `S(r,s,a)`. A default implementation calls bind and enters its result, but a replacement can perform different post-evaluation behavior. Explicit binding and direct entry remain separately available operations.

This offers the most explicitly separated policy surface among these candidates. It also needs a defined way to locate the send and bind policies themselves. The initial seed proposal placed them on vtable behavior and used an Id-like native meta-dispatch path to reach them. That was additional semantic structure, not a performance optimization.

| Property | A | B | C | D |
|---|---|---|---|---|
| Selection can directly inspect application receiver | No | Yes | Yes | Yes |
| Selected Method can inspect application arguments | Yes | Yes | Yes | Yes |
| A separate send hook sees evaluated arguments | No | No | No | Yes |
| Bind and lookup have independently replaceable identities | No | No | Yes | Yes |
| Ordinary entry is direct after selecting a Method | Yes | Yes | Yes | Yes for the default policy; custom send may choose another operation |
| Universal wrapper allocation is required | No | No | No | No; it depends on the implementation |

The table describes the proposed interfaces, not an implementation-cost ranking. More hooks may be optimized away under valid assumptions. A single unconstrained hook may still be expensive.

All four designs can expose ordinary objects implementing additional dispatch services. For example, B's lookup body can send a message to a replaceable Binder object. That can provide separate configurable policy at library level without making another native dispatch stage mandatory for every program. The fact that C names bind separately is therefore not by itself a proof of greater expressive power than B. A comparison must say whether the named hooks are primitive observations that have to remain unchanged, or library objects that a translation may introduce.

## 5 What an ordinary Method closure can express

Suppose lookup returns this Method:

```text
tracedMethod(receiver, arguments):
    recordStart(receiver, capturedSelector)
    result = enter(capturedUnderlyingMethod, receiver, arguments)
    recordFinish(result)
    return result
```

Lookup can select `capturedUnderlyingMethod` and capture it in the returned Method. The entry protocol remains unchanged. There is no implicit additional invocation lookup: the tracing Method is the selected Method, and calling the underlying Method is an explicit part of its body.

A similar closure can choose a target from the receiver's current state, transform arguments, dispatch to an interpreter, or enqueue work and return a Future. The scheduler, Future behavior, and foreign-call machinery must still exist; a dispatch hook does not provide those facilities by itself.

Delegation must have a concrete meaning. Calling the same ordinary send again with the same receiver and selector may simply re-enter the policy forever. A policy can instead retain a selected underlying Method, refer to an explicitly chosen predecessor policy, or use a defined delegation operation. None should be an undocumented native bypass of arbitrary behavior.

Neither A nor D automatically makes argument evaluation lazy. This discussion starts after receiver and argument evaluation. A send hook that declines to call a Method cannot undo effects already performed while computing arguments. Changing that requires thunks or a change to source evaluation semantics, which is a separate design decision.

## 6 A restricted equivalence result

Fix a selected send policy `P`. Assume it can execute using `(v,r,s,a)` and captured state. Define a Method closure:

```text
W(v,s,P).entry(r,a) = P(v,r,s,a)
L_P(v,s)           = W(v,s,P)
```

Then fixed sending gives:

```text
E(L_P(v(r),s),r,a)
    = E(W(v(r),s,P),r,a)
    = P(v(r),r,s,a)
```

This is a factorization of call behavior through a callable closure. Conversely, a programmable send policy can implement fixed sending by evaluating `E(L0(v(r),s),r,a)`.

For that algebra to imply an observational simulation, the following assumptions are needed:

- The carrier Method can express the policy body and accept the call's arity.
- Selection and carrier construction add no observable effects. If the original policy-selection process has effects, they are reproduced in the same order.
- The comparison observes application results, identities returned to the application, errors, divergence, and effect order, but not intermediate private Method identities or extra internal frames.
- Selection and entry occur in an execution segment without an intervening observable suspension or concurrent replacement. Extending the relation to concurrent execution requires a publication model.
- Recursive ordinary sends use the same translated interpretation, and delegation does not accidentally recurse into itself.

Under those assumptions, the added selection/entry steps are silent. The same policy body then runs from the same application state with the same inputs. This supports a restricted simulation, including recursive calls under a consistent translation. It is not a proof that arbitrary unmodified programs inspecting both runtimes cannot distinguish them.

There are genuine ways for the assumptions to fail. A user may replace Method construction with an effectful operation. A debugger may expose the carrier frame. A captured closure may keep objects alive longer, affecting observable finalization. A policy selection may suspend. Native allocation failure can distinguish an implementation that allocates a wrapper from one that does not. These effects cannot simply be declared irrelevant in a full language equivalence theorem.

The useful conclusion is narrower: **a separate send hook is not required merely to express arbitrary policy after argument evaluation, provided lookup may return an expressive Method closure.**

## 7 Why binding is still observably different

### A receiver-information counterexample

Take two distinct objects `r1` and `r2` with the same vtable `v`. Let `m1` and `m2` be distinct existing Method objects. A receiver-aware binder can implement:

```text
B(r1,s) = m1
B(r2,s) = m2
```

Compare the calls from identical application stores, under a deterministic policy and without implicit access to the caller or receiver through stack inspection or dynamic context. Plain lookup receives `(v,s)` in both cases. Its input and accessible state are identical, so it must produce the same result and effects. It cannot satisfy both required Method identities.

An alternating stateful lookup does not solve this: the client may choose either receiver on its first call. A carrier Method that chooses at entry can reproduce execution, but the result of explicit binding is that carrier, not the required existing `m1` or `m2`.

This establishes a strict difference in binding observations under the specified interfaces. It does not establish that independent bind is needed: B's receiver-aware lookup already supplies the missing information. Nor does it forbid A from using distinct per-object vtables, which changes the premise and the representation visible to reflection.

The counterexample also assumes the observed `bind` operation is exactly the fixed low-level binder in A. A program can define a different receiver-aware binding service using ordinary messages, or override a high-level `methodFor:` message if the language permits it. That can reproduce the example at the new API. It does not make the unchanged low-level binder receiver-aware, and it may decouple the high-level service from the selection used by ordinary sends. This is why the public binding contract must be specified before claiming a strict language-wide expressiveness ordering.

### Selection time versus invocation time

Consider a renderer whose backend can be CPU or GPU. The client explicitly binds `draw`, changes the renderer's backend, then enters the captured Method using that renderer.

```text
renderer.backend = CPU
m = bind(renderer, draw)
renderer.backend = GPU
enter(m, renderer, arguments)
```

This is pseudocode, not proposed Birnam assignment syntax. A receiver-aware selector can return the existing CPU Method during binding. A generic Method selected by A might instead inspect `renderer.backend` when entered and choose GPU.

Both are coherent designs. They make different promises about when the choice is made. Returning a closure is not automatically equivalent to capturing the earlier decision.

The rule that a captured Method retains its selected implementation does not freeze all reachable mutable state. If the selected implementation is a router that deliberately reads current state, that is its retained behavior. To promise an early choice, the selector must capture that choice, and it must have the necessary information when doing so.

### Send-only effects and direct invocation

Suppose a system requires three observations: ordinary sends are logged; explicit binding returns the same existing Method `m` without logging; directly entering that captured `m` does not log. An independently exposed send hook naturally distinguishes these paths.

A lookup-only implementation cannot obtain this exact combination merely by moving the logging into lookup or into a returned wrapper. Logging in lookup also affects explicit binding. Returning a logging wrapper changes the binding result's identity and makes direct entry of that wrapper log. The example assumes the underlying `m` stays unchanged and no hidden call-origin channel distinguishes operations.

This is a reason to specify the interception boundary. It is not a security guarantee: in a fully reflective, unrestricted system, another program may deliberately obtain and invoke the underlying Method.

## 8 What exactly is captured

The public binding contract should say what object a successful bind returns. Several interpretations are possible:

- The current dictionary Method.
- The actual Method chosen by arbitrary lookup, including a synthesized Method.
- A receiver-bound callable that additionally captures the receiver.
- A live reference that resolves the current behavior again whenever entered.

The first two differ whenever lookup synthesizes policy. The last interpretation changes the meaning of capturing a Method and would conflict with the current intent unless exposed as a separately named operation. A receiver-bound callable is also distinct from an unbound Method entered with an explicit receiver.

The working contract for A is the second interpretation: bind returns the actual selected Method. Dictionary inspection, selection, and direct entry are different operations. A synthesized Method is not required to masquerade as the underlying dictionary entry.

Another question is whether repeated binding must return the same Method identity. Arbitrary lookup could construct a fresh closure each time. Requiring stable identities would need an interning or reuse contract; it does not follow from allowing closures. Conversely, treating fresh Method construction as unobservable would weaken reflection. The simpler dispatch proposal must choose rather than slide between those assumptions.

Snapshots and image loading make these choices persistent. A captured Method closure may retain a policy version, receiver, or underlying Method. Its image representation must preserve the declared graph relationships. Rebinding an image symbol must not silently turn a pinned Method into a live selector reference. Captured foreign handles require Alien's resource-restoration policy rather than copying process addresses.

## 9 Lookup effects and cache correctness

A cache can be correct about the selected Method and still be wrong about the program. Consider:

```text
lookup(v,s):
    counter.increment()
    return dictionaryMethod(v,s)
```

Reusing the result while omitting the increment changes behavior. The problem occurs with plain lookup, receiver-aware lookup, or a separate bind hook. Merely invalidating caches when a method dictionary changes does not preserve per-binding effects.

The distinction is between caching an answer and optimizing a computation. A compiler may optimize the body of effectful lookup while preserving the increment, or inline a selected tracing Method while preserving its logging. It cannot remove effects because the chosen target happened to stay the same.

Result dependencies must also be complete:

| Selection rule | Information needed to justify reusing a result |
|---|---|
| Ordinary dictionary/delegation lookup | Relevant dictionary, delegation, and lookup-behavior state |
| Selection based on receiver identity | Receiver identity plus other policy dependencies |
| Selection based on a mutable receiver field | That field's relevant state or a mechanism invalidating the assumption |
| Selection based on mutable global configuration | Configuration dependencies, even if receiver and vtable are unchanged |
| Selection with required effects | Preserved effects in addition to a valid result |

A receiver-aware interface does not force every fast path to include the receiver identity. A known lookup body that ignores the receiver can use the simpler dependency set. But neither compiler nor runtime may infer that property merely from the selector's spelling.

Inline caching and specialization are established techniques for reducing dynamic-dispatch overhead. The Self PIC work also uses observed receiver types to inform later optimization. This supports evaluating specialized fast paths; it supplies no measurements for Birnam's arbitrary custom lookup policies. [Polymorphic Inline Caches](https://bibliography.selflanguage.org/_static/pics.pdf)

The design should specify observable lookup effects before introducing cacheability annotations or a dependency-registration API. Otherwise those mechanisms risk redefining the language by implementation convenience.

## 10 The native base case and the proposed extra layers

Reflective dispatch must eventually reach machine code. Ordinary Id closes the lookup recursion with a native base case. Reifying additional hooks does not remove the need for an entry convention and some way to find the first implementation.

The earlier proposal used functions named `findControl` and `controlCall` to locate the replaceable send and bind Methods. Structurally, those functions perform the job of the original native binding/sending mechanism. The proposal then places the new policy Methods above them. Its distinctive path is:

```text
native meta-dispatch
  → selected send policy
    → selected bind policy
      → selected lookup policy
        → selected application Method
```

Replacing the application send policy does not replace the native path used to locate that policy. This boundary may be acceptable, but it must be explicit. Calling it “metaobject traversal” does not make it an optimization or eliminate its semantic role.

For every candidate, the kernel contract must state what is fixed: the Oop/vtable link, how a callable Method is entered, the root lookup base case, and any representation assumptions needed to read that base table. A requirement that every aspect of sending be replaceable without any fixed invocation agreement is stronger than these designs provide.

The useful question is which fixed rules programmers need to escape in practice. Lookup-only Id fixes the select-then-enter skeleton but allows the selected Method to implement substantial policy. Independent hooks expose more named control points while still relying on a deeper entry mechanism.

“Fixed” here means that the operational skeleton does not consult a separately replaceable language hook at that stage. It does not mean a native module's implementation can never be rebuilt or replaced. Replacing native glue through a compatible module/ABI mechanism is a separate capability with its own code-lifetime and caller-invalidation requirements. It should not be conflated with replacing a Method consulted by the next ordinary send.

## 11 Performance implications

There is no justified blanket ranking such as “plain Id is native-speed” or “replaceable bind is slow.” The following costs are separable.

**Dispatch depth.** A naive implementation of C or D performs more policy selections and native calls than A. If the current policies are known and their dependencies tracked, some of that structure may be specialized away. It is still work the unoptimized seed has to perform unless a simpler path is implemented.

**Additional information.** Passing the existing receiver to lookup need not allocate another object. The expensive part may be what the policy does with it: reading state, constructing a closure, or making cache validity receiver-dependent. Conversely, a default receiver-aware lookup can ignore the parameter.

**Method allocation.** Allowing closures does not require allocating one for every call. Normal lookup can return an existing Method. A policy may reuse a closure if that preserves identity, capture timing, effects, and lifetime. Reuse is not automatically safe merely because the closure's code is unchanged.

**Argument transport.** The seed ABI proposal used a Vector of arguments for a uniform all-Oop entry. Its packing, allocation, and access costs are independent of whether selection uses A, B, C, or D. An Oop-only message contract does not by itself require a freshly allocated Vector per send. Arity-specific entry conventions or valid compiler specialization are separate alternatives to assess.

**Invalidation and recovery.** Supporting replacement requires preserving assumptions about targets and any inlined lookup/policy code. Self demonstrates optimized execution together with live changes and targeted deoptimization, including changes affecting inlined methods. That establishes a relevant implementation precedent, not a guarantee that ordinary C compilation automatically supplies the necessary recovery state. [Dynamic Deoptimization](https://bibliography.selflanguage.org/_static/dynamic-deoptimization.pdf)

**Memory retention.** Policy closures and inline-cache entries can retain receivers, Methods, and executable code. Conservative collection may retain additional objects. A faster warm call benchmark is incomplete if the implementation retains every discarded policy or code version indefinitely.

For Birnam's native-speed goal, the favorable common case is stable behavior that can be specialized to direct calls or inlined code. The difficult case is frequently changing, effectful policy with broad dependencies. That tension exists with one hook as well as with three. Native artifact packaging does not change it.

## 12 Replacement and concurrency

The simple equations hide the interval between policy selection, application Method selection, and Method entry. A concurrent runtime needs to specify which publication each stage observes and how already-selected code remains valid.

For example, one worker can select a bind policy while another replaces lookup. It must be defined whether the selected binder deliberately consults the new lookup or retains an earlier selected dependency. Independent hooks create more such stage boundaries, but a closure that reads live state can also expose them in A.

The current Birnam direction distinguishes entered/captured implementations from future operations. A Method already entered may finish that implementation; a future send inside it must honor the behavior applicable to that send. A wrapper that captured an old underlying Method can explicitly enter it later, whereas a wrapper that performs a fresh send requests current selection. Those are distinct programs.

This suggests defining publication and capture contracts before claiming a concurrent equivalence theorem. One possible policy is to give each completed selection a clear publication point, retain its chosen implementation, and make subsequent selections consult current state. That is a candidate to evaluate, not a commitment that all dispatch stages form one atomic transaction.

Fiber suspension makes lifetime especially visible: a selected Method, its captures, and any active native entry must survive collection and code replacement. Odin-specific context must not become a hidden channel supplying a receiver to an otherwise receiver-unaware lookup. If a fiber-local dispatch context is intentionally introduced, it should be an explicit language/runtime contract and included in the comparison.

## 13 Design implications for a small bootstrap

The first implementation milestone is an Odin interpreter running a Birnam-authored PEG module. That milestone needs coherent replacement, captures, Oop-only interaction, and a finite dispatch path. It does not itself provide a use case requiring every potential metaobject hook.

Choosing a smaller seed should not silently make the final language less malleable. Equally, an untested layer should not be introduced solely because its name appears in a statement that “everything is replaceable.” A concrete example should identify the behavior that the layer enables and the simpler candidate cannot express under the intended observations.

The alternatives impose different obligations:

- A requires a clear contract for synthesized Methods and for what explicit binding observes.
- B additionally requires receiver-aware lookup semantics and suitable cache assumptions.
- C requires separate bind/lookup replacement and reflection contracts, including delegation between the hooks.
- D additionally requires the boundary between ordinary sending, explicit binding, direct entry, and the native path that finds the send policy.

None dictates the argument carrier, native callable envelope, optimizer layout, or a second semantic object system. These should remain separate decisions so a performance problem in one area is not attributed to another.

## 14 Evaluation that would inform the choice

First compare semantic witnesses, with the observed result written down before implementation:

| Witness | Question answered |
|---|---|
| Two receivers sharing one vtable select different existing Methods | Must receiver-sensitive selection be visible at binding time? |
| Capture a Method, change receiver configuration, then enter it | Which choices are pinned and which are deliberately made later? |
| Ordinary send, explicit bind, and direct entry under tracing | Which operations should invoke interception policy? |
| Repeated binding of a selector whose lookup synthesizes a closure | Is Method identity stable, fresh, or implementation-dependent? |
| Effectful lookup with a warm cache | Does optimization preserve effects as well as the target? |
| Replace a lookup or policy used by an inlined send | Does the next affected operation use valid behavior without effect replay? |
| Suspend after selection, replace behavior, collect, then resume | Are publication and code/capture lifetime coherent? |
| Persist and reload a captured policy closure | Are capture semantics retained across image reconstruction? |

For performance, use equivalent ordinary behaviors across the candidates: an unchanged dictionary method; a reused tracing closure; per-receiver selection; changing receiver state; and explicit bind followed by repeated direct entry. Measure cold and warm behavior, allocations, retained memory, call throughput, and invalidation cost. Include both policies that can be specialized and policies that cannot safely be result-cached.

A microbenchmark of fixed cached targets cannot justify skipping arbitrary lookup effects. Conversely, comparing a freshly allocated closure on every A call with a preselected Method on every C call would conflate allocation policy with the dispatch design. The native C/Odin boundary and argument transport must be held constant or reported separately.

## 15 Working recommendation and review questions

The smallest promising candidates are **A, plain Id with expressive Method results**, and **B, receiver-aware lookup with the same fixed sending skeleton**. A is sufficient for a wide range of ordinary-call policies. B preserves the ability to make receiver-sensitive choices at binding time without introducing a separate bind hook.

C and D remain valid candidates if independent stage identity, interception, or reflection is a requirement in its own right. They should be justified by examples that survive the closure-factorization argument and cannot be expressed by B under the desired observation contract. They should not be justified merely by renaming the native Id machinery or by assuming that more hooks automatically mean more useful expressive power.

The most valuable review questions are:

1. Is the restriction of A to vtable/selector information a useful simplicity boundary, or an unnecessary limitation for a prototype-based language?
2. Should explicit binding expose the actual selected Method, including policy closures, or promise an underlying dictionary Method identity?
3. Which receiver-sensitive decisions must happen during binding rather than during entry? Are per-object vtables an acceptable alternative for those cases?
4. Does B lose any essential behavior of an independently replaceable bind once both have the same information, apart from the separate hook's reflective identity and composition boundary?
5. Is there a compelling send-only policy that must leave binding and captured direct entry untouched? What should a debugger show in that case?
6. Are lookup side effects fully part of the contract? If so, what evidence may justify caching a result or eliminating a lookup execution?
7. What is the minimal acceptable native base case? Which proposed “mechanical” rules actually carry semantics users need to replace?
8. Where should method selection linearize relative to publication, suspension, and entry? Is a per-stage contract sufficient?
9. Which observations should a real equivalence proof include: Method identities, logical frames, construction effects, finalization, or concurrent histories?
10. Which small implementation or counterexample would most change the assessment of A versus B versus independent hooks?

The decision is not whether Birnam can compute a particular function. It is where the language makes policy choices observable, when those choices are captured, and how much fixed machinery is acceptable beneath a malleable object system.
