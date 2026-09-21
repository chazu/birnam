# Collector v2

Historical public layout. [Collector v3](collector-v3.md) preserves this extension
contract and adds production activation and collecting constructors.

Collect publishes a 48-byte capability through the existing
[collection-state v2 owner](collection-state-v2.md). It performs tracing and
collection before Runtime exists, using the completed Id, Lifetime, Object Model
and Trace owners. `trace` and `collect` preserve the [base collection sequence](collector-v1.md).

`extend(context, extension)` installs a complete later owner's roots and
retirement behavior. Before installation the collector performs base collection;
there is no missing provider. After installation every public call, including
calls through descriptors copied earlier, invokes the installed extension.
Runtime installs its extension only after its Execution objects exist.

The 24-byte extension has a context and two synchronous operations:

- `trace(context, base, input)` supplies its mandatory roots and calls base tracing.
- `collect(context, base, input, out_count)` supplies its mandatory roots and
  complete retirement sequence, then calls base collection.

The collector copies the extension descriptor. Its context and native code must
outlive use of the collector. Installation requires the owner thread and Idle
phase. An exact repeated installation succeeds; another extension is rejected.
Nested collection is rejected before entering the extension. All callbacks obey
the existing trusted native contract and must not unwind.

The base is a scoped 40-byte v1 descriptor. Its operations bypass extension
dispatch so the extension can call them without recursion. The extension must
not retain the base, input, or output pointers. Base collection copies its input
and retirement table only for the synchronous call. This is a native composition
interface, not a language-level bypass of required roots or retirement.

Runtime's extension establishes active Runtime access for the call and emits its
roots before any extra caller roots. It supplies dispatch-cache invalidation,
Execution roots, deferred settlement, executable-binding retirement, deferred
reclamation and pruning. Runtime owns the complete upper retirement sequence;
a public caller can add scoped roots but must leave `input.retirement` null.
That field remains available to the extension when calling the scoped base.
Normal Runtime collection supplies only its captured native interval. Trace
probes supply only their additional probe roots. Neither can omit the installed
owner's obligations by using an earlier copy of the public capability.

The [C mirror](fixtures/collector-v2/collector_v2.h) fixes the new layout. The
outer collection-state descriptor remains 128 bytes; its independently versioned
collector pointer must now identify v2. The temporary Runtime constructor is
`birnam_runtime_init_v9`, preventing an old upper implementation from adopting
the new contract.

The native loading harness exercises saved entries before and after extension,
additional caller roots, root release, every retirement callback, exact repeat
installation, rejection of replacement and nested collection, and saved entry
use after actual Runtime adoption. Its separate trace-failure process checks
that a failed trace performs no reclamation.

Collector activation is still separate work: this change does not establish the
disabled/adopting/active transition, validate the startup heap, or install early
GC-aware construction methods. Those remain required by Design 83 section 5.
