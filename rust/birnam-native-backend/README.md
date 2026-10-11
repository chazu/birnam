# Native backend

`libbirnam_native_backend.dylib` initializes with one completed Namespace Object,
returns NativeBackend and publishes NativeBackend/NativeBackendVt. Reinitializing
with the same Id, Namespace and creating thread preserves the receiver and methods.
It owns emission algorithms; source interpretation/metadata stay with Codegen,
TCC owns compilation and mappings, and Execution owns callable/frame lifetime.

| NativeBackend message | Inputs and result |
| --- | --- |
| `emit:source:filename:persistent:synchronous:` | Canonical portable-program String bytes, provenance source String, filename String, two Booleans. Returns generated C String or `nativeEmissionRejected` Error. Persistent synchronous roots are refused. |
| `factory:source:dispositions:` | Canonical factory program String bytes, source String, Vector of Booleans in implementation preorder. Returns generated C or Error. |
| `named:synchronous:` | Integer arity and Boolean mode. Returns named-adapter C or Error. |
| `descriptorFor:` | Selector String bytes. Returns the exact named-adapter descriptor bytes in a String or Error. |

Wrong arity/types, foreign thread use and dependency message failures use typed
Id outcomes. Semantic emission failures use `Error kind: #nativeEmissionRejected`
with the retained emitter diagnostic. Inputs and intermediate projections are
rooted across collecting sends; byte borrows are copied before another send.
Descriptor symbols and native contexts remain mapped for process lifetime.

Backend IR, slot coloring and descriptor construction are implementation details.
The backend currently uses existing pure compiler helpers for portable lowering;
no Rust Program/CST/IR or Codegen state crosses its native interface. Native
emission and its C prelude are private to this crate. Frontend plans contain
canonical request data, so candidate/image inspection performs no emission.

Source compilation, source artifacts, named adapters, portable materialization,
candidate factories, image rebuild and generation descriptor hashing all call
this owner through current Id lookup. Compiled/mapped artifact and image loading
continue to consume saved metadata and objects without invoking it.

Resumable call-window optimization uses explicit option bit 32; the default
compiler mask is 125. Bit 64 additionally folds up to 32 trailing local copies
into the call batch; mask 61 disables that extension. Mask 29 keeps the earlier synchronous passes without this
optimization. The backend emits bounded source-slot metadata and retains the
complete original call-preparation sequence after a refused attempt. Execution
owns admission and frame mutation through
`tryPrepareCall:slots:selector:in:tail:`; the backend never inspects frame storage.
See [Execution's contract](../birnam-execution/README.md#guarded-generated-call-preparation).
