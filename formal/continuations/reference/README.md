# Executable continuation-aware Id reference

This is the first executable rebuild milestone. It implements the proposed
transfer semantics with a small Id library and independently compiled consumers
for Execution state, scheduling and test Methods. It is not a production Id,
native ABI, compiler or alternative installed runtime.

```sh
# First produce a current full formal receipt (see ../README.md for tool options).
python3 scripts/check-continuation-model.py --fetch-tools
python3 scripts/check-continuation-reference.py
```

The second command runs the Rust tests and compares synchronous/asynchronous
completed executions with TLA+ completion witnesses. The projection compares
Method entry, receiver, selector and arguments. It omits internal continuation
entries and the captured bootstrap lookup helper used by the custom lookup
fixture. Its nine entries agree in each configuration; this is a finite trace
comparison, not an implementation-refinement theorem or equivalence for every
reflection/allocation observation.

## Concrete entry decision

The primitive continuation is a captured **Method Oop**. Its context and saved
state belong to its owner. Answering enters that Method directly with the answer
as an Oop argument. The driver does not classify it as a continuation or decode
its payload. Requiring another lookup to deliver every lookup result would
reintroduce infinite metacircular recursion at the return boundary.

Captured continuation code retains its established meaning, just like an entered
Method. It can send fresh messages when resumed. A test replaces an ordinary
continuation object's behavior after constructing its captured entry and confirms
that the entry observes the new behavior through its explicit send. This does not
introduce an implicit application lookup on ordinary selected Methods.

The Rust `Transfer` enum describes control requests, not language values or Oop
payload kinds. Method arguments and language answers are Oops. Its representation,
the Rust callable declarations, and the construction helpers are reference-model
notation; none is a selected C ABI or permission to restore Operand/Outcome.

The generic allocation records hold only a vtable. Bootstrap Tables and Methods
are separate owners of their records. Execution fixture state is outside the Id
library, and the consumers cannot access Id's private records. There is no shared
private resolver between Id and Execution and no paired-library identity check.

## Acceptance and remaining work

The tests cover bootstrap publication, mutable root lookup, custom lookup that
calls another Method before selecting its target, pause/resume, synchronous and
async native-shaped callbacks, ancestor barriers, duplicate/foreign completion,
current versus captured Methods, DNU argument forwarding, continuation Oops as
data, two interleaved Machines and a 10,000-call tail chain. Poisoning lookup on
the selected Method does not prevent its direct entry.

The tail chain retains one native Method-entry activation. This does not prove
bounded total memory: allocations and native declarations are retained until
the reference Id is dropped. There is no collector, address reuse, native mapping
retirement or safe late foreign-pointer reclamation here. Rust closures and owned
argument vectors provide reference-model storage, not the production ownership
implementation. The helper bodies are Rust fixtures representing language
Methods; no Birnam source is compiled by this reference.

The next gate is the real Id calling convention with root handoff, code lifetime
and an independent native caller, followed by compiler/Execution reconnection.
The current production custom-lookup tail regression remains mandatory and red.
This reference should remain a semantic oracle; it must not become a second
production dispatcher or a compatibility path.
