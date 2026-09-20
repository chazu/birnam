# Lazy Error value policy v3

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
can select independent APIs and capabilities. The native wrappers synchronously
lend the selected caller's existing import slot pending native handoff migration;
that loan reads the slot only when invoked. Existing reentry, GC, live dispatch,
negative presence flags, arguments, results and image encoding are unchanged.
