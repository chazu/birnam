# Host support

Local Rust helpers for scoped Id sends, copied value projection, creating-thread
identity, bounded file input and owned command transport DTOs. This is an rlib,
not a native module or language owner. It owns no loader, runtime, scheduler or
foreign object representation. Native Host owners and the executable share Id's
actual Namespace and Roots; each reports its active native reference lists to GC.

Native owner initialization, receiver/argument retention, creating-thread checks
and Error construction are implemented here as local code in each dylib. Selector
lists and behavior remain with their respective native owners. The executable
uses the same value/root helpers but does not link owner implementations.

Each initialized Context retains one zero-payload tracing object. Its native
implementation reports references from active scopes through `referencesOf:do:`;
Id and Collect do not interpret Host state. The local weak index identifies only
these tracing objects and never keeps a Context alive. Closing scopes removes
references directly, in any order. Context teardown detaches its tracer and
releases the object. There is no token or release callback per result.

During initial callback setup, scopes still use Roots tokens. Native modules
declare the tracing callback in their existing module descriptor; the executable
adds it to the TCC declaration already used for bulk reads. This is implementation
bookkeeping, not language ownership or borrowing.
