# Birnam invariants

This is the authoritative statement of the language's design requirements.
Explicit user decisions can change it. Implementation choices, test expectations,
benchmarks and historical documents cannot silently add requirements to it.
The [architecture](ARCHITECTURE.md) describes the current implementation; the
[documentation index](docs/README.md) separates current references from history.

## Malleability and message passing

Birnam is a malleable object system. Objects interact through messages. Preserve
current method lookup, delegation, replacement, reflection and their observable
effects. An entered call uses the implementation it selected; subsequent sends
observe the current behavior. Explicitly captured Methods retain their meaning.

Ordinary dispatch is:

```text
send(receiver, selector, arguments) -> current method lookup -> enter that Method
```

Do not insert an implicit `apply`, `invoke`, or equivalent second lookup on the
selected Method. Explicit messages and control operations remain explicit;
returning an object, including a continuation, does not implicitly execute it.

## Minimal Id

Id's message receivers, selectors, arguments and results are Oops. Id does not
need to understand the language's data representations. Generic allocation, dispatch and
collection must not classify payloads using a fixed catalog of language types.
Allocation size, identity and reference encoding are not payload meaning.

Methods implement the behavior of their objects and interpret those objects'
representations. Other objects are accessed through messages, including messages
that report references for collection. Native calling conventions must not add a
second runtime value system. Bootstrap implementations establish an initial object graph; they do
not restrict the kinds of objects that can subsequently exist.

## Implementation code is not another object system

A native method may call implementation helpers: for example, a loop that copies
bytes, a hash algorithm, or a parser routine. Multiple method implementations may
reuse that algorithm code. These calls do not substitute for messages between
Birnam objects or bypass their replaceable behavior.

A Rust crate or source-file boundary does not by itself require another service
object, protocol, registry or wrapper. Reusing helper code does not authorize
sharing hidden Birnam state or building a parallel private runtime behind the
message interface. Organize code around the work it performs.

## Whole values and sensible operation sizes

A Bytes object represents a chunk of bytes and can be passed in a single message.
Whole-buffer work uses bulk messages; genuine indexed access may read one byte.
Oop-only messaging does not require one message, allocation or wrapper per byte,
field or element. Payload opacity is not an instruction to decompose useful
operations into tiny messages.

Define replacement behavior at the operation being offered. A bulk copy and an
indexed read are distinct operations; neither must be implemented by repeatedly
invoking the other. Internal call counts and temporary representations are not
language guarantees unless explicitly made part of an operation's behavior.

## Correct execution and collection

Preserve observable identity, aliases, shared state, results, errors and the
language-defined order of effects. Objects and executable code must remain valid
while running or suspended computations need them. Collection, callbacks,
reentry, cancellation and errors must not create dangling references, use released
code, repeat completed effects or lose required cleanup.

GC and native integration are responsible for making this work. Birnam does not
acquire ownership types, borrowing rules or borrow checking from Rust. Do not
make programs manage implementation memory rules that are not language features.
Changing storage must preserve supported references and observable identity.

## Creating a Method is separate from installing it

Compiling code, mapping executable code into the process, and creating a Method
object do not by themselves install or replace methods in an object's dictionary.
Default Method construction must not change dispatch bindings or advance the
binding epoch. Do not create private metadata vtables with installed methods as
a hidden part of constructing each Method, or hide such edits by changing counters.

An explicit method definition, dictionary edit, or module initializer may install
methods. Mapping code and executing an initializer are different actions; an
initializer's explicit effects still occur. Replaced construction operations keep
their observable effects; purity does not authorize bypassing those replacements. A Method can exist or be explicitly
invoked before it is installed in any dictionary.

## Stackless execution, concurrency and optimization

Retain stackless execution, concurrency, optimization and deoptimization. Their
implementations must preserve the same language behavior and keep running and
suspended computations valid. Optimized execution must retain bounded scheduling,
cancellation and collection opportunities, and support the agreed inspection and
recovery behavior. Recovery resumes the remaining work without replaying effects.

Optimization may remove internal work and allocations. It need not reproduce an
unoptimized allocation history or automatic collection schedule. It must preserve
observable effects of replaced constructors and explicit collection operations.

## Simplicity and performance

Start with the direct representation and operation. Additional dispatch,
allocation, copying, bookkeeping or synchronization needs a concrete reason.
State which required behavior it preserves, and measure the cost where relevant.
Do not introduce machinery merely to mirror a host language's abstractions or to
preserve an obsolete implementation's internal steps.

Prefer complete, comprehensible behavior over accumulating adapters around a
broken design. Passing tests does not establish simplicity, architectural
compliance or acceptable performance. Tests should check agreed behavior,
including prohibited alternatives, without turning incidental implementation
details into new invariants.

## Independent bootstrap and implementation language

Birnam must build from source with declared non-Birnam toolchains, without a
prebuilt Birnam executable, image or module. Rust, Zig, C, crate boundaries and
particular runtime data structures are not language requirements. The long-term
goal remains an increasingly Birnam-implemented system; keep genuine cold-build
dependencies until a working replacement exists.

Before changing a core path, explain its behavior and the proposed sequence of
operations. Identify any new mechanism and why it is needed. Routine private
refactoring needs no separate permission. A real change to these language
requirements needs an explicit decision; do not manufacture a conflict from a
historical document or an implementation preference.

An open invariant violation blocks migration that depends on it. Documenting the
violation or passing other tests does not make that mechanism compliant.
