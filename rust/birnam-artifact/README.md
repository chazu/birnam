# Artifact inspection through Id

Artifact is constructed after the value and collection owners. The
native initializer takes one Namespace Object and returns the Artifact singleton.
Reload with the same Id, Namespace and thread preserves methods and diagnostics.
The adapter uses Id sends for dependencies, publication, roots and descriptor
storage. The existing pointer-free codecs and private file-inspection helpers
remain unchanged; inspection does not load or activate the inspected library.
Codegen imports the existing provider-free `birnam-artifact-codec` package so
Cargo feature unification cannot pull Artifact's native exports into Codegen.

`Artifact inspect:` accepts an exact String-family
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
Byte-provider application copies byte values before later callbacks. Vector lookup remains
live, and method replacement is observed through fresh dispatch.

`bash scripts/oop-id-slice-test.sh` runs the v2 module export audit, the unchanged
artifact codec tests (59 pass, one existing test ignored), and native integration.
Integration checks cover descriptor fields and list projections, signed invalid
indices, diagnostic reasons, adjacent native requirements, byte snapshots during
provider mutation, live Vector construction and collection while materializing
fields. These checks inspect data files; full Codegen activation, source, Host/CLI,
image and worker acceptance remain pending.

`inspectBytes:from:` inspects supplied byte-bearing contents with an exact String
path used for descriptor provenance and adjacent native requirements. It shares
the same parser, compatibility checks and diagnostics as `inspect:` without
reading the artifact file. Codegen uses this to inspect and relocate one snapshot.
This selector does not execute or map code. Generated-module admission remains Codegen
policy; this inspection port does not qualify activation of old executable artifacts.

The v2 interface uses Artifact as the Oop Method context. Temporary references use
one owner-local scoped list, with direct accumulator reset for Vector projections.
Per-call root-token loops, copied incoming arguments and borrowed byte carriers
are removed. The descriptor parsers and their binary formats are unchanged;
ObjectModel still owns allocation, slot access and tracing of the thirteen fields.
