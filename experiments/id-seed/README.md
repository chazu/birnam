# Standalone Id and upper modules — Designs 81F/81G/81H

Run from the repository root on macOS:

```sh
mise run id-seed-experiment
```

The driver builds a fresh source snapshot offline, runs debug and release,
checks dependencies/exports/mapped images, and runs five deliberately broken
seed/Integer/tracer variants against unchanged probes. It retains binaries, command logs,
source hashes and `receipt.json` under `.cargo-target/id-seed-experiment/`.
Rustfmt and Clippy with warnings denied are part of the run.

This is a separate Cargo workspace. It is not linked into installed Birnam.

| Crate | Product and responsibility |
| --- | --- |
| `abi` | C-layout native operands, handles and call helpers; no implementation state |
| `observation-abi` | Borrowed storage facts, payload descriptions and report carriers; no heap/tracing implementation |
| `seed` | Dylib owning allocation, Symbols, default behaviors and metacircular sends |
| `object-model` | Independently loaded dylib implementing private-vtable policy through the API |
| `integer` | Independently loaded dylib owning signed 64-bit payloads, constructors and checked arithmetic |
| `trace` | Independently loaded dylib computing reachability from declared storage facts, roots and payload slots |
| `probe` | Executable that loads the libraries and supplies assertions/test methods |
| `trace-probe` | Separate host with explicit roots, a Pair fixture and an independent live/dead oracle |

Implementation/test crates depend only on the experimental interface crates. None uses production
`birnam-id`, `RuntimeState`, GC, Machine, compiler or native module libraries.
The probe is a test host with native test callbacks, not a proposed siphon.

Integer loads directly after Id and runs before the object-model module loads.
The driver also runs from a directory containing only the probe, seed and Integer
artifacts. Integer has no dependency on the private-vtable library. The full run
then checks that the two upper modules compose. The driver verifies that the
native ABI source still matches its original 81F hash. 81H intentionally extends
the seed with a storage-observation message and phase guard; its original three
native entries remain unchanged.

The Integer factory supports `integerBehavior` and `fromWord:`. Instances support
`asWord`, `+`, `-` and `*`. Word operands/results carry signed 64-bit bit patterns;
default arithmetic returns fresh objects and leaves its inputs unchanged.
Overflow returns module-specific status `0x100` before result construction.
Provenance checks belong to Integer: reassociating an unrelated object with
Integer behavior does not authorize reading its payload. Result construction
and allocation remain ordinary replaceable sends.

The seed has three callable API entries: send, behavior read, and reassociation.
The five seed methods remain ordinary replaceable methods; removal and parent
read/write are additional default-behavior methods for this witness. Roots and
selector handles are explicit. The ABI is experimental, not a claim that every
handle/entry must survive in the eventual minimal boundary.

`withStorageView:` on Vtable supplies exact allocation extents and seed-owned
roots/edges during a guarded observation callback. Upper modules supply their
own roots and payload-slot descriptions. A separate trace process loads its
tracer after the graph exists and tests exact reachability, errors and phase
guards. Unknown nonempty payloads fail instead of silently losing references.
This phase rejects ordinary sends and captured seed mutators; trusted callbacks
must also refrain from raw payload/module mutation. Root lists are explicit,
not automatically scanned from the native stack.

Calls use one owner thread. Storage, Symbols, method code and method data remain
alive until seed destruction; reachability reporting performs no reclamation or code retirement. Allocated
payloads have word alignment. Native operands are trusted borrowed pointers or
entry/data pairs, not boxed Birnam values. Seed construction or upper-method
publication failure aborts; no transactional installation protocol is claimed.
Destroy the seed before releasing consumer state, and release state before
unmapping code. This experiment's default private-vtable policy assumes behavior
responds meaningfully to `delegated`; arbitrary custom resolvers need not do so.

See [Design 81F](../../docs/design/81f-isolated-id-seed-proof.md) for the measured
result, mutation controls, exact scope and outstanding integration obligations.
[Design 81G](../../docs/design/81g-integer-module-proof.md) records the Integer
protocol, independent arithmetic oracle and extended isolation evidence.
[Design 81H](../../docs/design/81h-storage-observation-and-tracing.md) records the
observation contract, tracing proof and remaining reclamation obligations.
