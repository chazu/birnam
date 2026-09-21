# Runtime compiler extension v1

`libbirnam_runtime.dylib` has a second native initialization request, loaded through
Id with two operands: `Bytes("compiler")`, `Word(runtime_api_address)`. The API
must be the existing Runtime's API and the Id carrier must match its original
context, receiver and message operation. Bad inputs return `InvalidRequest`.
The request constructs artifact admission/catalog state and numeric/native
optimization registries. It returns `Loaded(Word(descriptor_address))`; repeating
the request returns the same descriptor without replacing state.

Production invokes this request after values, collections, Store, Artifact, PEG,
Lang and TCC, immediately before Codegen. Codegen refuses initialization when
this extension is absent. Base Runtime supplies Machine, native Block calling,
managed turns and GC without these registries. Optional optimization simply
declines while the extension is absent.

The 32-byte descriptor uses C layout:

```c
struct RuntimeCompilerV1 {
    uint32_t version; /* 1 */
    size_t byte_size; /* 32 on the supported 64-bit ABI */
    void *context;
    void *(*resolve)(void *context, const uint8_t *name, size_t length);
};
```

Only the native initialization result supplies the descriptor; the old compiler
provider export is removed. The descriptor and resolved functions remain
live with Runtime's pinned image for process life. Calls use the Runtime owner
thread. Names are exact byte spans, without a terminating NUL; unknown names
return null. The resolver exposes the fifteen registration, artifact admission,
generation lease, catalog and optimization operations listed in
[`runtime_compiler.rs`](../rust/birnam-abi/src/runtime_compiler.rs).
Existing function signatures and caller lifetime obligations are unchanged.
Codegen resolves those operations only through its supplied descriptor. A missing
descriptor or a null resolver result remains unavailable: neither triggers
provider discovery or a raw-symbol fallback. Its unit fixture uses a symbol
callback that rejects either fallback and checks both cases.

Base and extension code intentionally share a library, as Design 83 permits.
Legacy native exports remain for existing ABI consumers; compiler operations
require extension construction before use. This interface is a construction
boundary, not an attempt to prevent trusted native code from misusing exports.

The independent installed C fixture checks absence of the retired provider,
malformed and foreign-carrier rejection, complete symbol resolution, repeated construction and
ordinary Machine/value/GC use before installing the extension. Full source,
compiled and image checks exercise actual admission through Codegen.
