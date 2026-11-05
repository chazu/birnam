# Seed invocation proposal

> Under reconsideration: [dispatch equivalence analysis](dispatch-equivalence.md) proves a restricted ordinary-send expressiveness result and gives a counterexample to unrestricted equivalence. The extra replaceable send/bind layers below remain a proposal; no implementation should assume they have been accepted.

4 November 2026. Concrete proposal for the first interpreter milestone, not a ratified permanent ABI or implemented API. [Architecture](../ARCHITECTURE.md) defines the requirements; this document isolates the remaining dispatch choices so they can be reviewed before code.

## Required operation

```text
ordinary send
  → current send behavior
  → current bind behavior
  → current lookup selects a Method
  → enter that exact Method
  → return an Oop
```

The metaobject operations needed to find those behaviors require a finite native base case. They must not become a second general-purpose dispatch path that ordinary compiled/interpreted sends use to bypass replacement. There is no implicit `apply`/`invoke` message after selection.

## Proposed C boundary

Use an incomplete C object pointer as `Oop`. The proposal uses an ordinary Vector of arguments for the uniform seed entry:

```c
typedef struct BirnamObject *Oop;
typedef Oop (*BirnamEntry)(Oop method, Oop receiver, Oop arguments);

Oop birnam_send(Oop receiver, Oop selector, Oop arguments);
Oop birnam_bind(Oop receiver, Oop selector);
Oop birnam_enter(Oop method, Oop receiver, Oop arguments);
```

Names and signatures are proposed. All parameters and results are Oops, including the argument carrier. There is no Odin context, native value union, raw argument-count parameter, or Rust outcome enum at the message boundary. The native entry address is implementation machinery associated with a Method; it is not an extra language-visible value kind.

`birnam_send` follows current replaceable send/bind behavior. `birnam_bind` explicitly asks for the currently selected Method. `birnam_enter` invokes an already-selected Method without another dictionary lookup; native adapters, the seed evaluator, and future generated code use the same calling contract. Errors are Oop values. Host I/O and collector callbacks can have their library-mandated C signatures internally; Alien/collector adapters translate at the Id boundary.

The Vector carrier is proposed for an easily inspected, arity-independent seed. It has real allocation and access costs and must not quietly become a permanent requirement for every optimized send. Future call specialization may eliminate internal packing where semantics allow; it must still honor observable send/bind replacement. Compare with fixed-arity Oop entries before freezing a shipped artifact ABI. No near-native performance claim follows from this proposal.

## Candidate metaobject placement

Proposed defaults live on the behavior of vtables, so the receiver of the control methods is the application receiver's vtable:

| Proposed selector | Receiver | Arguments | Result |
|---|---|---|---|
| `send:to:with:` | receiver's vtable | selector, application receiver, argument Vector | Application result Oop |
| `bind:to:` | receiver's vtable | selector, application receiver | Selected Method Oop or Error |
| `lookup:` | receiver's vtable | selector | Method Oop or lookup failure |

The default send behavior asks that vtable to bind, then directly enters the selected Method. The default bind behavior asks it to look up the selector. Custom vtable behavior can replace these methods. The application receiver is passed explicitly so binding can depend on more than its behavior identity. The selector names are proposals, not imported facts about Piumarta's implementation.

For the native base case, propose a Piumarta-style internal meta-dispatch routine. `RootMeta` below is the self-referential vtable of vtables; `behavior` reads the minimal mechanical vtable link. `args(...)` denotes an argument carrier, not source syntax:

```text
findControl(receiver, selector):
    vt = behavior(receiver)
    if receiver == RootMeta and selector == #lookup:
        return primitiveTableLookup(vt, selector)
    lookupMethod = findControl(vt, #lookup:)
    return enter(lookupMethod, vt, args(selector))

controlCall(receiver, selector, arguments):
    method = findControl(receiver, selector)
    return enter(method, receiver, arguments)

ordinarySend(receiver, selector, arguments):
    return controlCall(behavior(receiver), #send:to:with:,
                       args(selector, receiver, arguments))

default #send:to:with: on a vtable:
    method = controlCall(self, #bind:to:, args(selector, receiver))
    return enter(method, receiver, arguments)

default #bind:to: on a vtable:
    return controlCall(self, #lookup:, args(selector))
```

For readability this omits propagation of lookup/arity Errors; an Error result must propagate rather than be entered as a Method. A captured-Method entry bypasses `ordinarySend` intentionally because its selection has already happened.

For an ordinary object whose vtable has `RootMeta` as its behavior, finding `send:to:with:` reaches `findControl(RootMeta, #lookup:)`. That primitive table search retrieves the currently installed lookup Method. It is then used to find the current send Method on `RootMeta`. The send Method receives the application vtable, obtains the current bind Method through the same mechanism, and binding obtains the current lookup Method to search the application vtable. Finally the selected application Method is entered directly.

This separates a narrowly scoped metaobject traversal from ordinary sending: `findControl`/`controlCall` are native mechanics for reaching the replaceable control methods and are not alternate evaluator/compiler send paths. Meta-level lookup therefore does not recursively consult replaceable `send`/`bind` just to find those same controls. Replacing the root lookup entry is still observed because the base case reads the table, not a cached original Method. Custom metaobject cycles can fail to terminate; a broad recursion-depth fallback must not silently bypass their behavior.

This explicit unresolved point is preferable to implementing the old fixed native send and calling it replaceable because lookup alone is replaceable. Do not add a second implicit invocation protocol to resolve it.

## Method representation and evaluation

The bootstrap needs a minimal callable envelope identifying a Method's native entry and captured state. Its exact byte layout remains open. Interpreted Methods use an entry that evaluates a Birnam expression graph with the selected Method's captures and receiver binding. Native Methods use entries implementing their owner object's primitive behavior. Selection and lifetime are the same in both cases.

Persistent expression nodes and environments are objects whose evaluation/lookup behavior is reached through messages. Tokenization scratch and native loops over an owner's byte buffer can remain implementation helpers. The evaluator must not become a permanent native switch over every language value or expression kind that bypasses replaceable behavior.

Defining a method from a Block creates a Method that retains the Block's code and lexical environment and binds its first parameter to the message receiver. Installing it changes a dictionary; merely creating or explicitly capturing it does not. Replacement creates a new Method for future sends and keeps old captured/entered Methods usable. Invocation arity validation must happen before entering an incompatible native routine.

## Native allocation and lifetime boundary

Generic allocation establishes addressable storage; each implementation interprets its own payload. The collector module performs conservative, initially non-moving collection. The seed must preserve Oops in registers, native stacks, argument carriers, expression graphs, and captures according to the qualified collector integration. Do not hide the only pointer in unscanned Odin allocation, host maps, or opaque C-owned memory.

The callable envelope and vtable links need a compatible bootstrap representation, not a generic catalog of Integer/String/etc. Id may know the mechanical layout required to dispatch; it may not branch on semantic payload types. Odin-owned entry adapters establish Odin context internally. No Odin-specific layout or implicit calling argument leaks into the C declarations above.

## Acceptance required before the evaluator depends on this ABI

- A C caller uses only Oops and the reviewed C header to send and enter Methods; it requires no Odin context definition.
- Replacing a normal method changes the next send; a previously captured Method still returns the old result.
- Replacing `send`, `bind`, and `lookup` separately changes their respective supported operations. A custom lookup is not bypassed by the seed dispatch path.
- Looking up a Method enters it once. Adding or replacing an `apply`/`invoke` method on that Method does not affect ordinary sends.
- Creating a Method leaves dictionary bindings unchanged until installation.
- GC inside a nested call retains the receiver, arguments, selected Method, captures, and return value.
- The root meta-dispatch case terminates while observing current bindings; arbitrary nested metaobject calls do not trigger a depth-based shortcut.
- Incorrect arity and missing-method paths return defined Error objects rather than crossing an incompatible native ABI or crashing accidentally.

The review decisions are whether to adopt this argument carrier and control-dispatch boundary, the proposed selector placement, and the callable-envelope layout/ownership. The sequence above is a concrete candidate, not a claim that the user has approved these implementation details. In particular, the boundary between native metaobject traversal and replaceable ordinary sending must be reviewed against the malleability contract before it becomes code.
