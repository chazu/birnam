# Runtime glossary and wording rules

Use these terms in explanations, documentation, code comments, reviews, and commit
messages. Prefer the concrete operation over a label for an implementation layer.
Explain unfamiliar terms when first used. Existing identifiers, quoted historical
text, and recorded measurement keys may retain their original spelling; explain
what they mean rather than silently rewriting evidence.

## Preferred wording

| Avoid in new prose | Use instead | Meaning |
| --- | --- | --- |
| behavior query | **vtable lookup** | Finding the vtable that determines an object's behavior. This may involve a private allocation-record check; it is not necessarily another Id message. |
| retained-name call | **interning an existing Name** | Interning a Name that was created earlier and is kept alive. In a benchmark, explicitly say that creation is outside the timer. |
| allocation-table lookup | **reading the allocation record** | Looking up the object's allocation size, identity, or storage information. This reads existing bookkeeping; it does not allocate an object. |
| method-record lookup | **reading Method data** | Obtaining a Method's code, context, and code owner. If a separate table is involved, say **looking up Method data in a separate table**. |
| name-owner lookup | **checking Name storage** | Checking that the payload satisfies the Name implementation's format before reading its bytes. This does not require a constructor-origin tag. This is distinct from checking its current vtable: changing behavior does not resize an object. |
| invocation-root references | **objects kept alive for a call** | The receiver, arguments, selected Method, context, and owner that must survive any GC during the call. **Call roots** is acceptable after this definition. |
| census delivery / allocation census | **listing allocated objects for GC** / **allocated-object list** | Supplying the collector with the objects that currently occupy managed storage, including the facts it needs about them. Say whether this uses callbacks, a snapshot, or both. |
| retirement preparation / metadata retirement | **removing dead objects from runtime tables** | Removing records that must not outlive an object, before its address can be reused. This is separate from freeing its memory. |
| native message interface | **Id message interface** | Id is the message interface. Native code uses a particular calling convention to implement it; that is not a second messaging system. |

**Vtable lookup** and **method lookup** are different: the first finds an object's
vtable; the second searches the applicable vtables for a selector's method. Do not
use either term for the whole message send without saying so.

## Other terms

- **Oop:** a reference to a Birnam object. A supported immediate representation is
  also an Oop and participates in Id dispatch. An arbitrary integer or native
  pointer is not automatically a valid Oop.
- **Vtable:** an object describing behavior through method bindings and delegation.
- **Selector:** the name identifying the requested method, normally a Symbol.
- **Live dispatch:** choosing the method using the current bindings and lookup
  behavior. Already captured or entered Methods retain their established meaning
  and code lifetime; future sends still observe replacement.
- **Malleability:** the ability to replace and rewrite behavior and implementation
  through the language's supported protocols. A storage optimization must not
  quietly freeze a binding or bypass a supported replacement.
- **Method:** the implementation selected by message lookup. `send` enters it
  directly; there is no separate application protocol. The bootstrap owner stores
  its code declaration index, context and code owner and interprets that format.
- **Send:** the message operation: find the receiver's current method for the
  selector, then enter that selected implementation with Oop arguments/results.
- **Context:** the object supplied to a method implementation for its own state.
- **Code owner:** the object that keeps the selected executable code and associated
  resources alive while they are needed.
- **Failure return / caught error:** a native failure return marks an object
  reference. Catching it yields a normal reference to that object, which can be
  stored or returned as data. Rethrowing marks the return again. The marker is
  not an allocation type or an intrinsic property of the error object.
- **Name:** an object containing immutable bytes to be interned. Names have variable
  length. A benchmark's 32- or 64-byte name is an input size, not a representation
  limit.
- **Interning:** finding or creating the canonical Symbol for a name.
- **GC root:** a reference from which the collector starts finding live objects.
  Registering a root records a reference; it does not copy the object.
- **GC / garbage collection:** finding live objects and reclaiming objects that
  are no longer reachable under the runtime's lifetime rules.
- **Marking:** following references from roots to identify live objects.
- **Sweeping:** reclaiming the identified dead objects, with the required table
  cleanup and finalization ordering.
- **Finalization:** performing an owner's required cleanup before storage release.
  State the supported lifetime and reentry rules rather than implying resurrection
  or ordinary use of a dead object is supported.
- **Phase timers:** diagnostic clock readings around parts of an operation, such
  as listing objects, marking, or freeing storage. State whether they measure CPU
  or elapsed time. These are measurement tools, not runtime abstractions. If phases
  overlap or one includes another, do not add their times together.
- **Calling convention / ABI:** the binary rules that let implementations call
  one another, including argument and result representation. Distinguish these
  rules from Id's language-level message semantics.
- **Stackless execution:** keeping logical execution state and continuations in
  explicit runtime-owned frames rather than depending on the native call stack
  to preserve a suspended language computation.
- **Operand / Outcome:** names of the old non-object structs in Id's calling
  convention. Operand carried a tagged native value; Outcome added native status
  and detail to a returned Operand. Use these names when discussing that code,
  not as generic names for objects or results in the Oop-only design.

## Communication rules

1. Name the work: for example, “three allocation-record reads per send,” not
   “boundary validation overhead.” Distinguish a private helper from an Id send.
2. Distinguish a necessary responsibility from its implementation. Keeping live
   arguments safe is necessary; copying them into another list may not be.
3. Distinguish readings, reference copies, host allocations, and Id object
   allocations. They are different costs.
4. Report what a measurement includes and excludes. A faster call-only benchmark
   does not prove a faster language or application.
5. Introduce new terminology only when it distinguishes an actual concept. Prefer
   ordinary words, established project terms, and exact code identifiers.
