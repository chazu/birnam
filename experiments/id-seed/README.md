# Standalone Id and upper modules — Designs 81F/81G

Run from the repository root on macOS:

```sh
mise run id-seed-experiment
```

The driver builds a fresh source snapshot offline, runs debug and release,
checks dependencies/exports/mapped images, and runs three deliberately broken
seed/Integer variants against the unchanged probe. It retains binaries, command logs,
source hashes and `receipt.json` under `.cargo-target/id-seed-experiment/`.
Rustfmt and Clippy with warnings denied are part of the run.

This is a separate Cargo workspace. It is not linked into installed Birnam.

| Crate | Product and responsibility |
| --- | --- |
| `abi` | C-layout native operands, handles and call helpers; no implementation state |
| `seed` | Dylib owning allocation, Symbols, default behaviors and metacircular sends |
| `object-model` | Independently loaded dylib implementing private-vtable policy through the API |
| `integer` | Independently loaded dylib owning signed 64-bit payloads, constructors and checked arithmetic |
| `probe` | Executable that loads the libraries and supplies assertions/test methods |

All four implementation/test crates depend only on `abi`. None uses production
`birnam-id`, `RuntimeState`, GC, Machine, compiler or native module libraries.
The probe is a test host with native test callbacks, not a proposed siphon.

Integer loads directly after Id and runs before the object-model module loads.
The driver also runs from a directory containing only the probe, seed and Integer
artifacts. Integer has no dependency on the private-vtable library. The full run
then checks that the two upper modules compose. The driver verifies that both
the seed and ABI sources still match the original 81F hashes.

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

Calls use one owner thread. Storage, Symbols, method code and method data remain
alive until seed destruction; there is no collection or code retirement. Allocated
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
