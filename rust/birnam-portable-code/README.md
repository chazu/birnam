# PortableCode through Id

Bootstrap loads the five ordinary prototypes: CodeNode, CodeStatement,
CodeProgram, CodeLexicalBinding and CodeObject. The native initializer takes one
Namespace Object and returns Unit. Reload with the same Id, Namespace and thread
preserves installed methods and namespace replacements.

Construction, inspection, errors, values, collections, allocation, roots and GC
connections use Id sends. The module owns immutable native records, following
the existing owner-map convention; no foreign payload is read. GC obtains edges
through `referencesOf:do:` and `visit:` sends, and releases records through
`finalize:`. Temporary values are retained across sends, and foreign String bytes
are copied before another send. No native visitor callback or service table remains.

The language selectors, diagnostics, zero-based indices, immutable clone behavior
and PortableProgram-v1 encoding are preserved. `floatBits:` and `digest` still
use binary Strings. Constructors use the receiver's current behavior; decoded
families, Error and Vector are resolved through the live Namespace. Tree traversal
remains iterative. The existing provider-free Artifact codec supplies validation
and canonical encoding without linking Artifact's module initializer.

`scripts/native-message-test.sh` audits the dylib and runs the production
`portable_code` integration test. It checks all thirteen frozen program digests,
inspection/reconstruction, constructor errors, collecting replacements, native
failure propagation, derived prototypes, GC edge retention/reclamation and reload.

`CodeObject canonicalBytesOf:` exports an admitted object's canonical program as
Bytes; `fromCanonicalBytes:` validates Bytes and constructs a CodeObject with nil
provenance. Both are ordinary messages. Inspection overrides do not redefine the
owner's canonical program. Execution now owns Block `codeObject`, `captureCount`
and `captureAt:`; [Codegen](../birnam-codegen-runtime/README.md) materializes both
synchronous and resumable Blocks. The `materialization` production test exercises
these connections. Image and optimization remain separate; the retired native
Block admission hook is not restored.

`CodeObject parameterCountOf:` and `captureCountOf:` accept one admitted CodeObject
Object and return native Words. They read checked immutable state without exporting
canonical bytes, allocating Objects or sending dependency messages. Invalid arity,
carriers or non-admitted Objects produce native `INVALID_REQUEST`. Both remain
ordinary replaceable Id methods; the caller keeps the supplied Object live under
the native ABI contract. Execution uses these queries for Block construction and
pending image shells. Canonical export remains for compilation, images and other
real byte consumers.
