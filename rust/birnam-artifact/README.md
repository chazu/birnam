# Artifact inspection through Id

Bootstrap loads Artifact after the value/collection and Execution owners. The
native initializer takes one Namespace Object and returns the Artifact singleton.
Reload with the same Id, Namespace and thread preserves methods and diagnostics.
The adapter uses Id sends for dependencies, publication, roots and descriptor
storage. The existing pointer-free codecs and private file-inspection helpers
remain unchanged; inspection does not load or activate the inspected library.
Codegen imports the existing provider-free `birnam-artifact-codec` package so
Cargo feature unification cannot pull Artifact's native exports into Codegen.

The public API is unchanged: `Artifact inspect:` accepts an exact String-family
path and returns a descriptor or nil. `lastFailure` retains the existing Symbol
diagnostic and resets to nil after successful inspection. Existing size limits,
ABI/target checks and adjacent native-requirement checks remain in place. Native
message failures propagate unchanged. Descriptor accessors, zero-based list
access, counts and Vector projections retain their selectors and value types.
Symbol projections preserve the old first-NUL termination; String projections
preserve all bytes.

Descriptors retain their thirteen object slots, allocated, read and written
through ObjectModel with an all-reference layout. ObjectModel supplies the
existing clone and GC behavior. Artifact exposes `rootsDo:` and sends `visit:`
to the GC visitor. Each new descriptor edge is installed before another
constructor send; inputs and temporary list results are rooted across sends.
String byte borrows are copied before any further send. Vector lookup remains
live, and method replacement is observed through fresh dispatch.

`scripts/native-message-test.sh` includes the production `artifact` integration
binary and dylib boundary audit. Tests cover descriptors and all list projections,
diagnostics, collecting/replaced dependencies, typed native failures, transient
input/result lifetime, clone/MOP/reload, and compiled source. A real dylib with a
constructor sentinel proves inspection leaves native initialization untouched.
PortableCode, artifact activation, Image and Host/CLI migration remain separate.
