# Seed Invocation Contract and ABI Proposal

**Receiver-aware lookup selected for the seed, 5 November 2026.** The semantic contract below is agreed. Under the user's permission to choose either argument convention provided all arguments remain Oops, the seed uses a uniform C entry with an Oop argument Vector. Exact exported names, callable-envelope layout, selector spelling, and native recursion details still need implementation specification. No runtime is implemented yet. [Architecture](../ARCHITECTURE.md) governs this work.

This replaces the earlier layered send/bind proposal. The alternatives and their tradeoffs remain in [the dispatch review](dispatch-design-review.md) and [equivalence analysis](dispatch-equivalence.md).

## Agreed operation

```text
receiver and argument evaluation
  → receiver-aware lookup selects a Method or Method closure
  → enter that exact selection directly
  → return an Oop
```

Lookup receives the receiver's vtable, the selector, and the application receiver. The vtable is lookup's own receiver; the application receiver is an explicit additional argument. Default lookup searches own behavior and then a single parent chain. Throughout delegation, the eventual application Method's receiver remains the original object.

Lookup may return an existing Method or synthesize a Method closure carrying data. Both are callable Method objects with the same entry contract. Explicit binding returns the actual selected Method, not a promised underlying dictionary entry. Two objects sharing one vtable may therefore bind the same selector to different existing Methods.

Send and bind are mechanical glue; the seed does not require independently dispatched send/bind policy hooks. Sending follows current lookup and enters its selection. Capturing or entering a Method retains that selected implementation, while future sends observe the current lookup behavior. Captures can still refer to mutable objects: retaining an implementation does not freeze its reachable state.

There is no implicit second lookup of `apply` or `invoke` on the selected Method. Explicit Method/Block invocation remains an ordinary explicit operation. Returning a Block does not execute it. A lookup failure propagates as the defined failure outcome and must not be entered as a Method.

Receiver and argument expressions execute once, left to right with the receiver first, before application method selection. If an argument replaces the pending behavior, final selection must observe the applicable replacement.

## Semantic equations

```text
v(r)         = receiver r's behavior
L(v,s,r)     = receiver-aware lookup
E(m,r,args)  = direct Method entry

bind(r,s)      = L(v(r),s,r)
send(r,s,args) = E(bind(r,s),r,args)    // successful binding case
```

These equations specify the selected mechanism, not its caching or allocation strategy. Custom lookup may have effects and inspect receiver state. A cache or specialization must preserve all those observations and dependencies; `(vtable, selector)` alone is not generally a sufficient cache key. Repeated lookup is not required to synthesize a fresh closure, and this choice does not promise stable identity for arbitrary repeated synthesized selections.

## Native lookup base case proposal

A finite native base case is still needed to find lookup's own implementation. Propose the selector `lookup:for:` and the Piumarta-style self-referential root vtable. This is a concrete candidate for the mechanics, not another accepted policy layer:

```text
bind(receiver, selector):
    vt = behavior(receiver)
    if receiver == RootMeta and selector == LookupSelector:
        return primitiveTableLookup(vt, selector)
    return send(vt, LookupSelector, args(selector, receiver))

send(receiver, selector, arguments):
    method = bind(receiver, selector)
    if binding failed:
        return the failure
    return enter(method, receiver, arguments)
```

Here `LookupSelector` is the interned `lookup:for:` selector, `behavior` reads the mechanical vtable link, and `args(...)` denotes whatever argument convention is ultimately selected. The displayed failure branch is semantic notation, not permission to introduce a hidden native Error value kind.

For an ordinary object `r` with behavior `V`, and `behavior(V) = RootMeta`:

1. `bind(r,s)` sends `lookup:for:` to `V` with `(s,r)`.
2. Binding that lookup message sends `lookup:for:` to `RootMeta` with `(LookupSelector,V)`.
3. Binding this root lookup message hits the native table-search base case. Its current Method is entered to select V's lookup implementation, which is then entered on V with `(s,r)`.
4. V's lookup implementation returns the application Method. The original send enters it with `r` and the already-evaluated arguments.

The two receiver roles are deliberately different at each meta level. The primitive reads the current root dictionary entry; it does not pin the original lookup Method. Delegation inside the default application lookup keeps the original `r` argument while traversing parent behaviors. Custom lookup and metaobject cycles can still be nonterminating; a recursion-depth shortcut must not silently replace their semantics with native default lookup.

This proposal needs direct tests of root replacement and nested custom lookup before evaluator work relies on it. It eliminates the previous `findControl`/`controlCall` layer for independently replaceable send/bind policies.

## Seed C boundary

Use an incomplete C pointer type for Oop and a uniform argument Vector for the seed. This is an engineering choice for simple interpreter/native interoperability, not a permanent requirement to allocate a Vector at every optimized call:

```c
typedef struct BirnamObject *Oop;
typedef Oop (*BirnamEntry)(Oop method, Oop receiver, Oop arguments);

Oop birnam_send(Oop receiver, Oop selector, Oop arguments);
Oop birnam_bind(Oop receiver, Oop selector);
Oop birnam_enter(Oop method, Oop receiver, Oop arguments);
```

`arguments` is an ordinary Vector Oop containing the argument Oops. The Method, receiver, and result are also Oops. A source Vector passed as one argument remains that argument; the transport Vector contains its reference and does not replace or flatten it. Odin context, a native semantic value union, and Rust outcome enums do not cross this boundary. Native collector/Alien adapters may use the signatures required by their underlying libraries internally.

The Vector carrier simplifies arity-independent entry but can add packing, allocation, and access costs. Later native compilation may use direct, arity-specific Oop entries for known targets with a generic bridge where needed. Such paths must preserve lookup effects, replacement dependencies, argument identity, and any exposed carrier behavior. The seed choice neither proves the direct path faster nor freezes a permanent native artifact ABI. Oop-only interaction does not itself require a fresh transport Vector for every send.

The runtime owns carrier lifetime. Any carrier exposed or retained by a Method or suspended computation must remain valid; reusable storage cannot be overwritten while still observable. The convention introduces no language-level borrowing rules. Transport packing must not introduce observable construction behavior absent from the source; if its implementation depends on replaceable constructors, those dependencies/effects must be accounted for.

`birnam_bind` obtains the current selected Method through receiver-aware lookup. `birnam_enter` directly enters an already-selected Method. A C caller must require no Odin-specific knowledge. Exact exported names, arity-validation mechanism, and callable-envelope layout/ownership remain open.

## Method representation and lifetime

The bootstrap needs a compatible callable envelope identifying a Method's native entry and captured state. Interpreted Methods use an entry that evaluates a Birnam expression graph with captures and receiver bindings; native Methods enter their owner implementation. Both obey the same selection and lifetime contract.

Defining a Method from a Block retains its code and lexical environment and binds its first parameter to the application receiver. Creating a Method does not change dictionaries. Installation publishes it for future sends. Replacement retains old captured/entered Methods and their needed code. Incorrect arity must not enter an incompatible native routine.

Persistent expression nodes and environments remain objects with message-based behavior. Native tokenization scratch and owner-local algorithms do not become a permanent semantic value/AST hierarchy bypassing replaceable behavior.

Collector integration must preserve live receiver/argument Oops, selected Methods, captures, expression graphs, and code. The only reference must not be hidden in unscanned native storage. Odin adapters establish their context internally and keep it valid for the duration of any suspended native frames. No generic catalog of Integer/String/etc. payload layouts is introduced into Id.

## Acceptance before evaluator dependence

- A C caller uses only Oops and the reviewed header, without an Odin context definition.
- Normal method replacement changes the next send; a previously captured Method retains its selected implementation.
- Two receivers sharing a vtable can select distinct existing Method identities through receiver-aware lookup.
- A synthesized Method closure is the actual result of explicit binding and executes with the supplied application receiver/arguments.
- Own behavior overrides parent behavior, and inherited Methods retain the original application receiver as `self`.
- Replacing lookup changes later binding/sending. Warm execution preserves its required effects and receiver-state dependencies.
- Ordinary sending enters the selected Method without consulting an `apply` or `invoke` method on that Method.
- Constructing a Method leaves dictionary bindings unchanged until installation.
- An argument that replaces the pending target affects the final send's selection.
- GC inside a nested call retains receiver, arguments, selected Method, captures, code, and return values.
- The native root case observes current root bindings and terminates in the valid bootstrap graph. Nested custom lookup is not bypassed by a depth-based fallback.
- Missing methods and incorrect arity produce defined Error objects rather than an accidental native ABI failure.

Dispatch and the uniform Oop-Vector seed entry are settled. Exact envelope/arity mechanics and the bounded native lookup implementation still need specification and tests. Arity-specific fast paths and the published artifact ABI remain later decisions.
