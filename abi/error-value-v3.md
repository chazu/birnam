# Error construction and legacy value policy v3

## Production Error construction

Bootstrap now loads `libbirnam_id_error.dylib:birnam_native_module_v1` before
Integer, Float, Block or Runtime. This is the installed form of `birnam-error`;
its [export manifest](rust-error-dylib-v1.json) records both filenames.
The entry receives Id and five Word operands: the completed root registry,
Lifetime, base Nil/Boolean values, Namespace and String operations descriptors.
It returns the `Error` factory as an Object operand, with no new descriptor.

| Receiver | Selector and operands | Result |
|---|---|---|
| Error factory | `kind:message:` Object kind, Object message | Error containing those references |
| Error factory | `kind:` Object kind | Error whose message is nil |
| Error factory | `message:` Object message | Error whose kind is `#error` |
| Error instance | `kind` / `message` | Stored reference |
| Object / Error instance | `isError` | false / true |
| Error instance | Unbound message | Same Error, propagating the failure |
| Object | Unbound message | Error kind `#doesNotUnderstand`, with a String naming the missing selector |

The native methods have no Integer, Float, compiler or Execution dependency.
Malformed native operand counts/kinds return InvalidRequest. Construction roots
kind/message across the current Id allocation message, checks the allocation's
actual size and behavior, and writes the existing two-Oop payload. Readers check
allocation bounds and reference membership before returning fields, so a copied
method or changed behavior cannot expose a native payload word as an object.
Explicit native null Object arguments and uninitialized null fields denote nil.

Lifetime receives the Oop payload layout. Error registers its own canonical
roots and publishes `Error`, `ErrorVt` and `ErrorFactoryVt` into the existing
Namespace. Bootstrap then releases its temporary result root. Error fields and
String buffers retain their ordinary collection/finalization behavior. Runtime
preserves the already installed Object missing-message handler; late Error
mounting preserves identities, layouts and exact method/context bindings. The
legacy `id_init` path still uses the previous constructor. Printing and Block
control extensions continue to mount later with their dependencies.

The independent C fixture `scripts/fixtures/early-error-native.c` runs against
the actual thirteen-library prefix, then the same Runtime after adoption. It
checks construction, recognition, propagation, missing-message semantics,
malformed operands, payload safety, live factory replacement and precise GC of
an Error's kind and String message. No alternate Error implementation is used.

## Legacy Error Value v3 adapter

`birnam-error-value` remains the stateless adapter for consumers using the old
IdApi calling convention. It calls the real Error factory constructed above;
its provider and per-call dispatch contract remain unchanged. The lazy absence
behavior below still applies if a caller removes/replaces Namespace bindings or
uses legacy construction. This adapter does not own Error representation.

Design [81BD](../docs/design/81bd-production-error-import-handoff.md) moves the
shared consumer's import selection to a per-call
[Namespace import capability](namespace-import-v1.md). Compared with
[v2](error-value-v2.md), both operations gain `const NamespaceImportV1 *`
immediately after presence. The [C mirror](fixtures/error-value-v3/error_value_v3.h)
specifies the signatures. The immutable provider remains 32 bytes, version 3.

Hosts select `birnam_error_value_provider_v3`; all nine consumers attach through
`birnam_error_value_dependency_v3`. Null descriptors, wrong version/extent,
missing operations and retargeting reject; repeated attachment succeeds. There
is no old provider/dependency export or fallback. Initialization creates no
objects, roots, methods or layouts and invokes no callbacks.

Recognition short-circuits a null value without accessing either capability.
It then checks live ErrorVt presence; zero returns false without accessing import.
Otherwise it imports the current behavior and compares that result snapshot with
the subsequent vtableOf result. A raw-null imported behavior returns false.

Construction retains the existing borrowed kind/message extents, interns kind,
creates the String, checks live Error presence, then imports its factory. Zero
presence or raw-null factory returns the selected API's current nil. Otherwise
it interns kind:message: and sends to the imported factory snapshot. Presence,
import, selector and send callback order stays unchanged.

Neither operation reads IdApi.import or retains the capabilities. Earlier kind,
String or presence callbacks can replace import's function/context. Nested calls
can select independent APIs and capabilities. Native wrappers forward the selected
caller's explicit live import capability directly. IdApi v10 has no import slot.
There is no production slot loan or API-address lookup. Existing reentry, GC, live dispatch,
negative presence flags, arguments, results and image encoding are unchanged.
