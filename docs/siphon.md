# Native siphon slice

Implemented 5 November 2026. This is the first native bootstrap slice, not the seed interpreter or a completed Birnam runtime.

Follow-up: [the seed interpreter](seed-interpreter.md) is now available as a separately loaded module. The host's thin `--eval` option sends `evalFile:` to the current `Seed` object; it contains no parser/evaluator. Since 9 November 2026 the interpreter needs the separate [runtime services](runtime.md) loaded before it, and `--help` gives that order. The original native-slice qualification below remains separate from the new source-language evidence.

## Build and run

```sh
make doctor
make build
./build/siphon --help
make verify
./build/siphon ./build/hello.dylib
```

`make verify` builds the C test modules and the [native example](../examples/native/hello.c). The executable accepts native module paths, loading them in order into one world. Relative paths are relative to the caller's working directory. There is no module search path or source/image auto-detection yet.

[toolchain.json](../toolchain.json) pins the initial Mac build: Odin `dev-2026-10:84bc3fc21`, Apple Clang 16.0.0 build `clang-1600.0.26.3`, macOS SDK 15.0, and Boehm 8.2.12 with the exact installed library checksum. The build driver verifies these inputs and writes actual observations to `build/toolchain-observed.json`. Install the declared native tools/dependencies separately; the build neither downloads them nor invokes an old Birnam installation. A mismatched version or checksum fails explicitly.

The SDK is selected explicitly because this machine also has a newer Command Line Tools SDK that Clang 16 cannot link against. Changing a pin requires requalification; this is not a claim that every Boehm 8.2.12 package has the same behavior. The documented baseline runs on macOS 27.2 ARM64. Linux and other toolchain combinations are not qualified.

## Bootstrap and dispatch

The Odin executable creates one self-referential root vtable and ordinary objects for the namespace, symbol service, siphon, collector, basic native Bytes/call Vectors, Methods, and Error behavior. `Siphon` is an object in that namespace, and its `load:` behavior is replaceable. The same root namespace is passed to every module initializer.

The implemented public Id boundary is [birnam.h](../include/birnam.h):

```c
typedef struct BirnamObject *Oop;
typedef Oop (*BirnamEntry)(Oop method, Oop receiver, Oop arguments);
```

`birnam_send`, `birnam_bind`, `birnam_enter`, and `birnam_intern` accept and return only Oops. Bootstrap/behavior-anchor accessors return Oops. The native argument carrier is a Vector Oop containing the argument Oops; a source Vector passed as one argument is not flattened. Odin context never appears in this ABI.

Native construction helpers in [birnam_native.h](../include/birnam_native.h) are C implementation code, not a second Id message API. They allocate GC-managed native payloads and assemble the mechanical header and call-Vector layout. Their byte counts, C entry pointers, and raw payload initialization are local native construction machinery; messages still cross the ABI as Oops. Native modules can define arbitrary C payloads and interpret those payloads in their own Methods. No semantic type tag or catalog of language payload layouts is used by dispatch.

The mechanical header precedes the payload and records behavior, payload extent, native entry, and arity. A null entry denotes a non-callable object. The layout is checked in C and Odin and is a seed ABI, not a permanent published artifact format. The call-Vector adapter owns its count/element layout; it does not define every future collection's representation. Well-formed native Oops and ABI-compatible payloads are prerequisites; this is not a memory-safety boundary against arbitrary native code.

Binding follows the receiver-aware `lookup:for:` protocol. The reflective root case reads the current root table entry rather than a cached original Method. Default lookup searches the current behavior and then sends lookup to the parent, carrying the original application receiver. Sending enters the actual selected Method directly. There are no separate send/bind policy stages or implicit `apply`/`invoke` sends, and no lookup-result cache.

Generic entry validates the call-Vector extent, arity, null arguments, and null native results. Non-callable lookup results are inspected through `isError`; a declared Error propagates, while another non-callable result yields `notMethod`. This assumes the supported bootstrap graph retains a functioning Error protocol. Arbitrary incompatible edits to root lookup can still make meta-dispatch nonterminating, consistent with the malleability contract; there is no depth-based shortcut silently substituting native lookup.

## Available object protocols

| Receiver | Messages supplied in this slice |
|---|---|
| Ordinary object | `behavior`, `clone`, `define:as:`, `isError`, `==` |
| Vtable | `lookup:for:`, `delegated`, `parent`, `at:put:`, `clone` |
| Namespace | `at:`, `at:put:`, `clone` |
| Symbols service | `intern:`, `clone` |
| Native Bytes | `intern`, `loadInto:` |
| Symbol | `intern`, identity-preserving `clone` |
| Error | `isError`, `kind`, `description`, explicit reflection/identity operations, and propagation for other messages |
| GC service | `collect`, identity-preserving `clone` |
| Siphon | `world`, `load:`, `retain:`, `modules`, identity-preserving `clone` |
| Loaded module | `name`, `result`, identity-preserving `clone` |

Object-local `define:as:` creates a new behavior delegating to the previous one, so edits remain local even after a specialized object has been cloned. Explicit vtable `at:put:` changes shared behavior. This simple implementation accumulates delegation layers for repeated local definitions. Each new layer records its owner, and every operation that shares a behavior clears that record; the [authored Id](id.md) uses it to reuse an owned layer. `IdPrimitives` exposes the Behavior/Namespace storage operations the authored module builds on. Default cloning copies opaque payload bytes shallowly; native owners must override it when that is not their intended resource behavior. Vtable and Namespace clones copy their own binding lists while retaining referenced values.

The symbol service sends `intern` to the name object, allowing that object's current behavior to interpret its payload. Native Bytes implement exact-byte interning; Symbol interning returns the same canonical object. This avoids making generic Id decode arbitrary name objects.

Error propagation is supplied by Error's lookup behavior. Only explicitly supplied Error protocol operations handle it; an unrelated inherited Object method does not silently consume the Error. Full source-level recovery/sequence behavior remains evaluator work.

## Native module entry and lifetime

A library exports:

```c
Oop birnam_module_init_v1(Oop method, Oop world, Oop arguments);
```

The loader wraps this C entry in a zero-arity Method, passes the existing namespace as receiver and an empty argument Vector, and invokes it through `birnam_enter`. The result is recorded on an ordinary module object. An Error result fails the load. Initializers run on each load; dependency resolution, idempotent source-module loading, and initializer ordering beyond command-line order are not implemented.

`Siphon load:` delegates to the path object's `loadInto:` behavior. The native Bytes path implementation validates empty/NUL-containing paths, uses the platform loader, and invokes the versioned entry. Errors distinguish opening failure, missing entry, and initializer failure. The CLI reports the Error and exits nonzero rather than loading subsequent command-line modules.

Every successful native open remains mapped until process exit, including a missing-entry or failed-initializer case. OS constructors or an initializer may already have published code, so the loader cannot safely infer that an error permits unmapping. Retained module objects are reachable through `Siphon modules`. No rollback or immediate unload is provided. Missing-entry detection is not a guarantee that OS-native constructors had no effects.

The handover fixture replaces root lookup and shared behavior using messages in the existing world. A later module verifies that a pre-existing object observes new sends while a previously captured Method still executes old code after GC. This demonstrates compatible takeover mechanics. The production takeover is the separate [authored Id module](id.md).

## Memory qualification and limits

All seed allocations use the pinned Boehm library with interior-pointer recognition and ordinary non-incremental collection. The bootstrap root references are explicitly registered. Native call carriers and binding lists are GC-managed, so their Oop edges are visible to conservative scanning. Tests invoke collection from inside C Methods and retain old Methods across replacement.

`GC` exposes collection through messages. Allocation remains native bootstrap machinery, and a separately loadable collector implementation plus allocation-policy handover is not yet delivered. This first slice must not be described as a fully replaceable collector module. Finalization is on-demand; the reclamation test explicitly drains finalizers. There is no language finalizer API yet.

The C test workload registers 4,096 unreachable objects for finalization, collects between bounded batches, checks a conservative retention allowance, and bounds heap growth. This is evidence for this seed/build/workload, not qualification of fibers, incremental collection, blocking foreign calls, or every possible native register configuration. The previously researched `GC_do_blocking` path is not used here.

Out of scope remain source parsing/evaluation, Integer/String/Block libraries, authored PEG/Lang/compiler, source-slot reflection/accessor factories, images, fibers, thread-safe publication, Alien FFI facilities, optimized dispatch, and CBPV. The C fixtures supply their own native payloads/accessors to test the implemented object floor. No previous Rust code or artifacts are part of the build.

## Acceptance commands

`make verify` builds with Odin vetting and C warnings-as-errors, then checks:

- Independent C modules using the same Oop-only Id boundary.
- Receiver-aware and synthesized lookup, delegation, local overrides, clone independence, namespace copy independence, and current root lookup replacement.
- Entered/captured Method lifetime and absence of an implicit application lookup.
- Kinded failures, Error propagation including inherited-method cases, argument-carrier identity, malformed carriers, and arity checks.
- Nested-call GC survival, bounded reclamation, and post-handover code survival.
- Failed initialization with already-published code remaining callable.
- Missing library/entry diagnostics and the native example from another working directory.

Build inputs and the current receipt are described in [siphon validation](siphon-validation.md).
