# Does replaceable lookup subsume replaceable send and bind?

For a self-contained review of the alternatives and their implications, see [Lookup Binding and Sending in Birnam](dispatch-design-review.md). The argument below provides the narrower derivation and records the discussion that led to it.

4 November 2026. A scoped mathematical argument and counterexample, not a machine-checked proof or a verified implementation. This analysis reconsiders the extra dispatch layers in [the seed invocation proposal](seed-invocation.md); it does not silently amend the accepted architecture.

**Determination:** replaceable lookup plus expressive Method closures can implement the same post-argument-evaluation behavior for ordinary sends under the assumptions below. That is an expressiveness result. It is not unrestricted contextual equivalence when explicit binding, Method identity, lookup effects, or reflective machinery are observable. The proposed additional send/bind layers therefore need justification by those stronger observations, not by ordinary-call expressiveness alone.

## Reference model and observation boundary

The Piumarta closure model selects a closure by binding, then directly calls that closure with itself, the receiver, and arguments. Closures can carry arbitrary state and support interpreted or native implementations. This is the source basis for allowing a lookup result to contain executable policy rather than just designate a pre-existing dictionary method. [Piumarta and Warth, section 4.1, figures 16–19](https://tinlizzie.org/VPRIPapers/tr2006003a_objmod.pdf)

Let:

- `r` be an application receiver, `s` a selector, and `a` already-evaluated arguments;
- `v(r)` be its vtable;
- `L(v,s)` be lookup, returning a Method;
- `E(m,r,a)` directly enter the selected Method;
- `S(v,r,s,a)` be the behavior of an independently replaceable send policy for a vtable.

All operations may interact with object state. Their observations include returned object identity, Error outcomes, divergence, and the ordered externally visible effects of the operation. Timing, internal bookkeeping, and intermediate private Method identities are excluded only for the ordinary-send theorem. Native faults and implementation allocation failures are outside that theorem. Receiver/argument evaluation has already happened in both systems, so neither interface gains a special ability to suppress those earlier effects.

Model A (fixed send/bind glue):

```text
send_A(r,s,a) = E(L(v(r),s), r, a)
```

Model B (independently programmable send policy):

```text
send_B(r,s,a) = S(v(r), r, s, a)
```

The notation suppresses state threading. The construction below identifies the point at which the selected policy begins running; the same underlying policy runs from the same application state in both models.

## Ordinary-send simulation by a returned Method

Assume:

1. Lookup can return or construct a Method closure whose body is as expressive as a send-policy body. That Method receives `r` and `a` and can capture `v`, `s`, and the selected policy implementation/state.
2. The carrier Method has a valid entry for the call's arity. The fixed entry protocol does not impose an extra semantic restriction that prevents executing the policy body.
3. Selecting/creating this carrier adds no visible effects and its intermediate identity is not observed. Any source policy-selection effects must be reproduced in their original order. This construction directly covers a stable selected policy; it does not claim arbitrary reflective policy-selection effects vanish.
4. We compare ordinary call boundaries, not explicit binding or inspection of intermediate methods/frames. Start with a single-threaded execution segment with no observable suspension between policy selection and entry. Extending the result to concurrent publication requires a corresponding selection/linearization contract and is not proved here.
5. Policy code uses the same ordinary-send semantics recursively, translated consistently. Any call that delegates to underlying behavior has an explicit reference/path to that behavior; it must not accidentally resend itself forever.

For each selected `S`, define a Method closure:

```text
W(v,s,S).entry(r,a) = S(v,r,s,a)
L_S(v,s)           = W(v,s,S)
```

Then:

```text
send_A(r,s,a)
  = E(L_S(v(r),s), r,a)
  = E(W(v(r),s,S), r,a)
  = S(v(r),r,s,a)
  = send_B(r,s,a)
```

The extra selection/entry steps are silent under assumptions 1–4. After those steps, the same policy body executes with the same receiver, selector, arguments, and captured state. Its state transitions, effects, Error/result, or divergence therefore match. Recursive ordinary sends use the same construction, giving a compositional simulation for this restricted observation model. Conversely, Model B can implement Model A by defining `S(v,r,s,a) = E(L(v,s),r,a)`. Thus the models have equal expressiveness for these ordinary-call observations.

This is a closure conversion/currying argument, not proof that two existing implementations run the same instructions or expose the same objects. It also relies on Method closures being constructible without adding observably different constructor effects. If construction is itself replaced with effectful behavior, that must be accounted for or the assumptions no longer hold.

The consequence is useful: tracing before/after a call, receiver- or argument-dependent routing, proxy forwarding, choosing an interpreter, returning a Future, or declining to call an underlying method can reside in the selected Method. Lookup need not receive application arguments itself; its returned Method receives them. Scheduling facilities still have to exist to implement an asynchronous policy.

The wrapper is itself the selected Method. Entering it does not introduce a second implicit `apply`/`invoke` lookup. If its body explicitly invokes some other captured Method, that is part of that body's chosen behavior.

## Counterexample: observable receiver-dependent binding

The proposed `bind:to:` is given the application receiver. Plain lookup is given only the vtable and selector. This difference is observable before invocation.

Let distinct objects `r1` and `r2` share vtable `v`. Let `m1` and `m2` be distinct, already-existing Method objects with observable identities. A replaceable bind policy can define:

```text
B(r1,s) = m1
B(r2,s) = m2
```

Evaluate either bind starting from the same state. A fixed binder using only `L(v(r),s)` supplies exactly `(v,s)` to lookup in both cases. With no hidden caller/receiver channel, lookup cannot distinguish those inputs. Returning a stateful alternating sequence cannot satisfy arbitrary receiver choices from identical state either.

Returning a carrier Method that chooses between `m1` and `m2` when invoked can reproduce the call results. It cannot make the result of explicit binding be identity-equal to `m1` for one receiver and `m2` for the other. An observer that compares the bound Method with those existing objects distinguishes the systems.

This disproves unrestricted equivalence of the proposed receiver-aware bind extension and lookup-only binding under the stated interfaces. It does not prove that the extension is needed by Birnam. A per-object vtable, explicit custom binding message, or broader lookup protocol could express the distinction, but each changes the setup rather than proving the unchanged interfaces equivalent. Arbitrary stack inspection or a hidden “current receiver” variable would also add an information channel excluded from this proof.

## Other observations that require separate decisions

- **Method identity and reflection:** a policy carrier may be a different Method from the underlying dictionary entry. A complete equivalence proof must specify what `methodFor:`, equality, method inspection, and stack inspection expose. A lookup returning synthesized Methods is legitimate only if that is the declared binding contract.
- **Captured Methods:** capturing a carrier pins that selected Method implementation; capturing an underlying method pins that one. Reading mutable state through either is still possible. The plain model can preserve Birnam's entered/captured-Method rule, but it does not automatically reproduce every separation between send-only interception and direct Method invocation offered by an extra hook.
- **Effects of lookup:** if lookup logs, mutates, or otherwise acts on each binding, caches cannot merely return its previous answer. An optimization must preserve the complete observable behavior, not just the selected Method. The same applies to replacing a send hook.
- **Concurrency:** policy replacement between selection and entry needs an explicit publication contract. The single-threaded algebra above is not a proof about all thread schedules, safepoints, or callbacks.
- **Bootstrap recursion:** both designs need a native termination point for reflective lookup. The argument does not prove that arbitrary cyclic metaobject edits terminate or preserve runtime correctness.

## Recommendation for Birnam

**Recorded for consideration, 4 November 2026:** the user requested retaining the plain-Id suggestion with an explicit contract that lookup may return a method or a method closure. This is an interesting candidate, not an accepted replacement of the current dispatch requirement. Here a Method closure is a callable Method carrying captured data; the distinction does not introduce a second dispatch or implicit invocation protocol.

Prefer plain Id-style fixed mechanical send/bind with replaceable lookup, provided Birnam chooses this contract explicitly:

1. Lookup may return an ordinary existing Method or a synthesized Method closure.
2. That selected Method defines the operation and is entered directly.
3. Explicit binding returns that actual selected Method; it need not be identical to an underlying dictionary entry hidden behind a policy closure.
4. Capturing/entering a Method preserves its selected implementation. Future sends observe the current lookup behavior.
5. Correct optimizations preserve custom lookup effects and policy changes; arbitrary custom lookup is not automatically cacheable.

This retains the ordinary-call capabilities motivating the larger proposal and avoids adding independent send/bind dispatch layers by default. Ordinary lookup can return an existing Method directly, so no universal per-call policy-wrapper allocation is required. Specialized policies may have costs and require measurement.

Adopting this recommendation would revise the current architectural requirement that `send` and `bind` themselves must independently be replaceable methods. It does not follow solely from the proof and is not yet recorded as an accepted change. The independent argument-vector ABI proposal also remains open; this equivalence argument neither requires nor validates it.

## What receiver-aware binding adds

Under the compared interfaces, independently replaceable receiver-aware binding is strictly more discriminating at the binding boundary: it can return different existing Methods for different receivers sharing a vtable. Lookup keyed only by that shared vtable and selector cannot reproduce that result from the same state without another information channel. The extended binder can also implement ordinary lookup-based binding, so it contains the simpler model's binding behaviors and adds the counterexample above.

This does not imply greater computability or greater expressiveness for ordinary-call results under the closure simulation assumptions. In lookup-only Id, a returned closure can inspect the receiver and choose behavior when invoked. A custom vtable per receiver is another representation strategy, but changes the shared-vtable premise and its observable structure.

The practical distinction is selection time. For example, a receiver-aware binder could select the existing Method associated with a receiver's current backend; a captured Method then retains that selected implementation even if the receiver's backend changes later. A generic closure that rechecks the backend when invoked has different capture semantics. Matching the former requires arranging an equivalent selection/capture mechanism; returning any old closure is not sufficient.

Independent hooks also separate ordinary-send interception from explicit binding and direct invocation. Whether Birnam needs that separation is an API/semantics decision. Neither the expressiveness argument nor the counterexample establishes how many native dispatch layers are needed to implement the chosen interface efficiently.

The demonstrated binding advantage comes from receiver information, not the name `bind`. A receiver-aware lookup, `L(v,s,r)`, could implement `B(r,s)` directly and close this particular gap while retaining fixed mechanical send/bind. That is a third candidate extending plain Id's lookup interface; it is not adopted here and does not establish equivalence of every reflective observation of separately exposed hooks.
