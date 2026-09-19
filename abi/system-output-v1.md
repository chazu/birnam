# System output installation v1

Design [81AL](../docs/design/81al-production-system-output.md) moves System
print:/println: and their writer from Prelude into `birnam-system-output`.
The module depends on [Text View v1](text-view-v1.md) for borrowed printable bytes.
It loads after Text View and before Prelude; initialization enables the provider
without creating objects, methods, roots, layouts or owned buffers.

Attach the immutable, process-pinned descriptor through
`birnam_system_output_dependency_v1`. Its arm64 layout is 24 bytes: version 1 at
0, byte size at 8 and one install function at 16. Admission rejects null, wrong
version/extent, missing install and retargeting; repeat attachment of the same
pointer succeeds. The [independent C mirror](fixtures/system-output-v1/system_output_v1.h)
checks layout without using Rust headers.

Install receives two const IdApi pointers, the selected System behavior, and the
already interned print:/println: selectors. Prelude keeps System creation/adoption
and all twelve selector interns, then calls the installer at the original first
two binding slots. Use the current registration table for both method callbacks.
Retain Prelude's original execution pointer before either callback can invoke a
method or reenter; this pointer can differ from the first registration table.
Later installations keep the original execution selection. Use the supplied
behavior/selectors synchronously without reinterning or retaining them.

Each method reads live fields/callbacks through that retained table and passes it
to Text View. Invalid arity/carrier or unavailable printable bytes returns current
nil without output. Success writes the exact bytes, adds a newline for println:,
then flushes stdout and returns the original receiver, including null. The writer
still stops on write errors internally, and its outer caller ignores IO errors.
No new normalization, escaping, error object, byte copy or collection opportunity.

The native methods and descriptor stay pinned. Borrowed views keep their existing
storage/lifetime obligations. System representation, IdApi v7, image encoding,
public selectors and fresh printing lookup are unchanged. Run
`mise run system-output` and `scripts/system-output-image-smoke.sh PREFIX` for
focused debug checks and startup/source-free compiled/saved-image/baked execution.
