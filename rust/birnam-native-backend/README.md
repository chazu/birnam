# Native backend

NativeBackend emits C for the current Oop-only compiler and Execution runtime.
The [architecture](../../ARCHITECTURE.md) maps the implementation; the
[runtime contract](../../docs/contracts/runtime.md) describes continuing behavior.

`libbirnam_native_backend.dylib` initializes with one Namespace argument, returns
NativeBackend, and publishes that receiver under `NativeBackend`. Reinitialization
returns the existing ready receiver when the Id root and Namespace match.
Method entry checks the creating thread and Namespace context.

Codegen handles source interpretation and metadata. NativeBackend implements
emission algorithms; TCC compiles and maps the result; Execution keeps callable
code and frames usable. These are current implementation responsibilities, not
requirements to preserve this crate arrangement.

## Messages

The installed entries are declared in [src/lib.rs](src/lib.rs). Program, source,
and filename inputs are read through their `bytes` operation and the bulk byte
protocol. Generated C is returned as a String.

| Message | Current behavior |
| --- | --- |
| `emitSynchronous:` | Emit a canonical portable program using the synchronous entry path, empty provenance source, and `<compiler>` filename. |
| `emitResumable:source:` | Emit a canonical portable program with source provenance, persistent top-level bindings, and `<compiler>` filename through the resumable path. |
| `emit:source:filename:persistent:synchronous:` | Emit a canonical portable program with the supplied provenance, filename, and canonical Boolean flags. This operation rejects the combination of persistent and synchronous flags. |
| `moduleFactory:source:dispositions:` | Emit a factory with persistent top-level bindings and per-implementation Boolean dispositions. The root must be resumable. |
| `factory:source:dispositions:` | Emit a factory without persistent top-level bindings, with per-implementation Boolean dispositions. |
| `rootsDo:` | Supply references currently retained by this native receiver to a visitor through `visit:`. Used by root tracking. |

For factories, dispositions are read through `size` and `at:`; current callers
supply a Vector. True selects synchronous emission and false selects resumable
emission in implementation preorder. The list must match the implementation
count, contain at least one entry, and contain no more than 65,536 entries.
These limits describe current admission, not a new language-wide size rule.

Wrong arity, invalid flags, foreign-thread entry, or an invalid native context
return the native invalid failure. Dependency message failures propagate.
Emission rejection becomes `Error kind: #compileError message: ...`, with a
synchronous, resumable, source, or factory emission diagnostic. The exact checks
and emission choices are in [oop_emission.rs](src/oop_emission.rs).

The module also declares a `receiveByteValues` callback for the shared native
byte reader. Initialization installs it on the reader's receiving behavior rather
than as another NativeBackend operation. Inputs and intermediate values are kept
alive across collecting sends; native buffers are obtained through bulk messages.

## Compilation and call preparation

Portable-program decoding, slot coloring, descriptors, and generated C helpers
are private implementation work. No Rust Program/CST/IR or Codegen state crosses
this object's message interface. Calling a shared pure compiler algorithm does
not add a second object protocol.

Source compilation, artifact construction, portable materialization, factories,
and image rebuilding use this backend through current Id messages. Mapping
compatible saved code remains distinct from invoking the emitter.

Generated resumable calls send `Machine prepareCall:selector:` with an admitted
call plan. The default operation performs the ordered frame transfer and publishes
the call action without sending individual slot-access or call-site messages.
Replacing the preparation operation still changes compiled calls. The behavioral
reference is [Execution and optimization](../../docs/contracts/runtime.md#execution-and-optimization);
the implementations are [oop_resumable.rs](src/oop_resumable.rs) and
[Execution call preparation](../birnam-execution/src/oop_call_preparation.rs).

Old experimental call-window option masks and the removed named-method forwarding
operations are not current APIs. Their former descriptions are preserved in the
[2026-10-30 reference snapshot](../../docs/archive/2026-10-runtime-refactors/native-backend-reference-20261030.md).

## Generated helper and selector storage

Private prelude helpers are `static inline`. The retained TinyCC compiler emits
referenced helper bodies, following transitive calls and address-taken references.
This removes unused generated C helpers, not Birnam methods based on an assumption
that an application will never call them. Existing mapped images keep their saved
code; recompilation is needed to change emitted helper bodies.

The emitter collects distinct literal names and declares `SYMBOL_COUNT` from
that actual list. The [prelude](src/oop_prelude.c) stores the supplied Symbols in
a fixed array; generated initialization checks the supplied count before copying
them. This stores names, not Method selections or lookup verdicts. The old
64–512 estimated-capacity description belongs to the archived implementation.
