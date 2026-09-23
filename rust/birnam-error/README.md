# Error

`libbirnam_error` takes Namespace and returns the Error factory. Bootstrap loads
it after String. It uses completed Roots, ObjectModel, ObjectVt, String, true,
false and nil objects, and publishes Error, ErrorVt and ErrorFactoryVt. Repeat
initialization returns the published Error without replacing methods.

Error's representation is two ordinary reference slots: kind and message.
Allocation, field access and initialization use Object Model messages. Its
existing slot tracing and shallow cloning keep both fields alive; Error adds no
collector, native resource registry or foreign layout access. Instance roots keep
its family, dependencies and selectors live. No process/TLS state, cached foreign
methods or descriptor pointers are used.

| Receiver / selector | Arguments | Result |
|---|---|---|
| Error `kind:message:` | kind Object, message Object | New error. |
| Error `kind:` | kind Object | New error with nil message. |
| Error `message:` | message Object | New error with kind #error. |
| Error instance `kind`, `message` | none | Corresponding reference. |
| Error instance `isError` | none | true; Object's default answers false. |
| Error instance `doesNotUnderstand:` | selector and original native operands | Receiver, preserving propagation. |

Constructors normalize null Object operands to nil; native Words/Bytes are not
language references. Error inherits ordinary shallow `clone`/`new`, which allocate
a new identity and preserve kind/message references.

Object's `doesNotUnderstand:` copies the current selector's bytes via Id,
constructs `does not understand #<selector>` through String's current `fromBytes:`,
and sends Error's current `kind:message:` with #doesNotUnderstand. Replacements
are observed on the next send. Inputs and intermediate messages are retained
across allocating callbacks; borrowed Symbol bytes are copied before further
sends. Calls remain on the owning Id thread.

No Integer, Float, compiler, Execution or Runtime dependency is needed. Run
`mise run native-messages` for production construction, propagation, live factory
replacement, automatic collection and reference-lifetime coverage.
