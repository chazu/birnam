# Authored Id module

Implemented 7 November 2026. [modules/id.birnam](../modules/id.birnam) is the first fuller Id module from the [architecture](../ARCHITECTURE.md). It is written in Birnam and takes over by replacing Methods on the vtables the siphon created. Definition, cloning, and reflection policy run as Birnam code. The siphon keeps lookup, storage, and other machine primitives.

## Boundary

Default lookup stays native (decided 7 November 2026). Every step of compiled or interpreted Birnam is itself a send. Each send selects its Method through `lookup:for:`. An authored root lookup would need a lookup before its first operation could run. The siphon's only base case reads the root table, and that would find the authored Method again. Piumarta's Id avoids the regress with a method cache, which Birnam rules out because lookup may have observable effects. Adding a different native stopping point would change the kernel contract, so it is a separate decision. Authored lookup policies can still be installed on individual vtables, as before.

Namespace `at:`, `at:put:`, and `at:ifAbsent:` also stay native. Compiled code objects are Namespace-backed, and every generated node reads its children with `at:`. An authored `at:` would therefore need itself to run. `==`, `isError`, `behavior`, the Error protocol and its propagation, Symbols, Siphon, and GC are unchanged.

The module needs the Block, Method, and Reflect services in [runtime.dylib](runtime.md), so it loads after that library. It does not need the interpreter. The authored toolchain now loads it first: [compile.py](../tools/compile.py) lists `id` ahead of PEG, Lang, and the compiler. That means the interpreted bootstrap, both native generations, and the generated-C fixed point all run on the authored Id, and the [native bootstrap](selfhosting.md) rebuilds Id as its sixth module.

```sh
./build/siphon ./build/runtime.dylib ./build/seed.dylib --eval modules/id.birnam --eval program.birnam
python3 tools/compile.py modules/id.birnam --name IdModule -o build/id.dylib
./build/siphon ./build/runtime.dylib ./build/id.dylib ./build/program.dylib
make verify-id
```

## Protocol

Loading replaces 9 siphon Methods and adds 12 new ones. `[Id replacements]` answers the installed rows as `#[vtable selector Method]`. Loading again installs fresh Methods; it is reinstallation, not hot reload.

| Receiver | Replaced | Added |
|---|---|---|
| Object | `define:as:`, `clone` | `undefine:`, `localSelectors`, `respondsTo:`, `methodFor:` |
| Error | `define:as:` | |
| Vtable | `at:put:`, `delegated`, `parent`, `clone` | `at:`, `includesSelector:`, `removeSelector:`, `selectors`, `parent:` |
| Namespace | `new`, `clone` | `keys`, `includesKey:`, `removeKey:` |

- `define:as:` reuses the object's own private layer. The siphon's version adds a layer for every definition. Repeated definitions on one object now stay in one layer, and lookups no longer walk one layer per definition.
- `undefine:` removes the selector from the object's local layers, exposing inherited behavior. It answers `unknownSelector` when the selector is not defined locally, including when the definition now lives in a layer shared with clones.
- `localSelectors` answers the selectors defined in the local layers, in definition order.
- `respondsTo:` and `methodFor:` use the same selection as a send (`Reflect methodFor:on:`), so a custom lookup's effects run.
- Vtable `at:`, `includesSelector:`, `removeSelector:`, and `selectors` see only the vtable's own entries. `removeSelector:` answers the removed Method. Missing entries answer `unknownSelector`.
- `parent:` reparents a vtable, accepting `nil`. A cycle answers `behaviorCycle`, and a non-vtable answers `typeMismatch`.
- `at:put:` and `define:as:` answer `notMethod` for a non-callable value.
- Vtable `clone` copies the parent, the meta behavior, and the entries. A vtable with its own `lookup:for:` keeps that lookup in the copy, and later edits to either stay separate.
- Namespace `clone` copies its bindings (the values themselves are shared), and `keys` answers definition order. `removeKey:` answers the removed value or `unknownName`.

## Private layers and ownership

A siphon Behavior now records an `owner`: the one object a private layer was made for. Both the native and authored `define:as:` set it. An object's local layers are the run of layers, starting from its behavior, that it owns. Authored definition reuses the top layer when the receiver owns it, and otherwise inserts a new layer.

Any operation that makes another object share a behavior clears its owner. These are Object, Namespace, and Vtable `clone` and Namespace `new` in both the siphon and the authored module. After cloning, the original and the clone each receive a fresh layer at their next definition, so a local edit cannot leak into a clone in either direction. The owner reference and the object's behavior form a cycle that the tracing collector reclaims.

Native code that allocates an object directly with another object's private behavior must clear the owner itself, using `[IdPrimitives behavior: vt owner: nil]`. Otherwise later definitions on the owner would leak into the new object. Existing runtime, seed, and siphon allocations use shared vtables.

Layers created by the native `define:as:` before takeover remain, and the authored policy reuses the top one when the object owns it. Takeover does not compact existing chains.

## Storage primitives

`IdPrimitives` is a siphon object whose native Methods read and write the siphon's Behavior and Namespace representations. They check those representations and answer `typeMismatch` for other objects. They contain no definition or copying policy. For example, `behavior:at:put:` stores any value; the authored `at:put:` decides that it must be a Method.

| Area | Selectors |
|---|---|
| Behaviors | `isBehavior:`, `behaviorOf:`, `newBehaviorWithParent:`, `parentOf:`, `behavior:parent:`, `selectorsOf:`, `behavior:at:`, `behavior:includes:`, `behavior:at:put:`, `behavior:remove:` |
| Ownership | `object:behavior:`, `behavior:isOwnedBy:`, `behavior:owner:` |
| Objects | `copyObject:`, `isCallable:`, `if:then:else:` |
| Namespaces | `isNamespace:`, `newNamespaceLike:`, `keysOf:`, `namespace:at:`, `namespace:at:put:`, `namespace:includes:`, `namespace:remove:` |

`object:behavior:` only specializes: the new behavior must delegate to the object's current one, so every implementation already interpreting the payload remains reachable. `behavior:owner:` requires the owner to currently use that behavior. `behavior:parent:` rejects cycles because native lookup and these walkers follow parents without a bound.

`if:then:else:` answers its second or third argument, unevaluated, by identity with the canonical Booleans. The module branches with it instead of `ifTrue:ifFalse:`. `define:as:` and `at:put:` depend only on the primitives and Block evaluation. They can therefore repair a replaced Boolean protocol, as the seed corpus does after redirecting `true ifTrue:ifFalse:`. Loops in the reflection and copying Methods still use the library's Integer, Cell, Vector, and Block behavior. Arbitrary edits to those objects can disable such loops, consistent with the malleability contract.

## Acceptance

`make verify-id` (also part of `make verify`) runs [id_test.py](../tests/id_test.py):

- A source takeover, loaded twice, with [id_guard.c](../tests/id_guard.c) loaded three times. The first load captures the 9 replaced siphon Methods; the second checks that none is still selected and replaces each entry with an Error-producing sentinel; the last checks that nothing entered one. The [Id corpus](../tests/id/semantics.birnam), seed semantics and collections, and PEG semantics and replacement all run between the second and third loads.
- Objects, specializations, captured Methods, and identities created before takeover by [before.birnam](../tests/id/before.birnam). A separate unguarded run, [captured.birnam](../tests/id/captured.birnam), deliberately enters the captured siphon Methods.
- [id_contract.c](../tests/id_contract.c): native callers reach the authored policy through sends; 1000 definitions on one object use one layer; and 4096 discarded specialized objects are finalized within a heap bound.
- The natively compiled module, Id corpus, and compiler corpus, run on `runtime.dylib` without the interpreter, under the compiler guard that fails if `Seed` is bound, together with the compiler contract.
- Byte-identical generated C from the authored compiler with and without Id loaded.
- Loading the native module without `runtime.dylib` fails with `unknownName`; loading it after `runtime.dylib` alone succeeds.

The recorded results are in [Id validation](id-validation.md).

## Limits

There is no source slot/accessor factory, authored lookup on the default path, method cache, thread-safe publication, module unloading, or compaction of existing layer chains. The siphon still supplies initial dispatch, allocation, loading, Errors, and the collector interface. Toolchain modules took 16–23% longer to compile in the recorded bootstrap, likely from authored `new`, `define:as:`, and Namespace `clone` on heavily used paths. That is an observation, not a controlled comparison, and the cost has not been profiled.
