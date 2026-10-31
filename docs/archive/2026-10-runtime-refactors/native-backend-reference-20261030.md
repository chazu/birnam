# Native backend

> Snapshot archived 2026-10-30 from `rust/birnam-native-backend/README.md` at
> `11e7a7e2123c616af4581b1bfc05a0f0cf7b9597`, before documentation repair.
> This preserves superseded instructions and API descriptions, including stale
> claims and links. It is historical evidence, not current requirements or a
> work queue. Use the [current reference](../../../rust/birnam-native-backend/README.md) for new work.

NativeBackend emits code for the integrated Oop-only compiler and Execution
runtime. See the [architecture](../../../ARCHITECTURE.md) and
[runtime reference](../../contracts/runtime.md). Older milestone reports
record the checks performed at their revisions, not current unfinished work.

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

Current Oop resumable emission sends one ordinary `prepareCall:selector:` operation
with admitted call plans. Plans are regenerated from portable IR and admitted
atomically by Execution when constructing the Method. Execution preserves current
frame reads/writes and call-site publication; an entered failure is not replayed.
The historical option controls below describe earlier experiments (prepared calls
used bit 128 and the default mask was 253).

Legacy resumable call-window optimization uses explicit option bit 32;
mask 125 retains the previous emission. Bit 64 additionally folds up to 32 trailing local copies
into the call batch; mask 61 disables that extension. Mask 29 keeps the earlier synchronous passes without this
optimization. The backend emits bounded source-slot metadata and retains the
complete original call-preparation sequence after a refused attempt. Execution
owns admission and frame mutation through
`tryPrepareCall:slots:selector:in:tail:`; the backend never inspects frame storage.
See [Execution's contract](../../../rust/birnam-execution/README.md#guarded-generated-call-preparation).

Private prelude helpers are `static inline`. The retained TinyCC compiler emits
these bodies only when referenced, following transitive calls and address-taken
references to a fixed point. Generated entrypoints, initializers and unit-local
state keep their existing definitions. Every
retained helper body and Id send is unchanged; the C compiler does not remove
Birnam methods merely because an application has not called them. Guard refusal
and recovery branches still reference—and therefore retain—their helpers.

This is private code-emission elimination, not a new speculative dispatch path.
Fresh source artifacts and images are required to obtain the size savings;
existing mapped images keep their saved code. Tests inspect real TinyCC ELF
symbols, require transitive helpers, and call a helper through a stored function
pointer. See `docs/startup-analysis.md` for paired rebuilt-prefix measurements.


Selector tables now choose a compile-time capacity from the emitted C's distinct
string-literal spellings: twice the estimate, rounded up to a power of two and
clamped to 64..512. Comments and character literals are skipped; escaped quotes
stay inside their string. Non-name strings and unused helpers can overestimate
demand. Malformed lexical input uses 512. This estimate is a storage policy,
not an admission proof: unknown demand and full tables keep the existing interning
fallback. The standalone prelude defaults to 512 when no capacity is supplied.

Hashing, spelling comparisons, canonical Symbol ownership and current Id dispatch
retain their existing contracts. Rooting failures clear the unpublished cache
entry. No method entry, lookup verdict or factory result is cached. Internal
interning frequency is not preserved, as documented in
[fixed names](fixed-names-and-local-protection.md). Capacity
selection introduces no runtime resizing, new native service or GC-budget change.
