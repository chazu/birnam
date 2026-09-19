# 81Q — Float methods above Id

> **Status:** Implemented. First Float checkpoint, following the collector in
> `87cb68bc` and the two-stage Integer extraction in 81L.

## Scoped extraction

Move the existing seventeen Float methods out of Prelude into `birnam-float`,
loaded after Integer and before object-model/Artifact. The new module uses the
existing Method and module-init interfaces and explicitly acquires Integer for
mixed arithmetic and conversion. It depends only on the no-state ABI crate.
There is no new native numeric optimizer declaration: the existing declaration
contracts apply to the Integer subset only.

The first checkpoint deliberately preserves the Id-owned Float canonical
behavior/prototype, payload, constructor, accessor, exact recognizer and all
current consumers. It adds no IdApi field and changes no image/Frame bytes. A
later checkpoint must migrate that representation and those consumers before
calling Float fully extracted.

## Invariants and callers

Methods move without algorithm changes. Preserve mixed Integer arithmetic and
ordering, Float-only equality, Error propagation except equality, wrong-arity
nil answers, fresh boxes, negative zero/NaN/infinity behavior, divide-by-zero and
sqrt domain errors, signed-64 conversion bounds and existing printing. Error
factory lookup remains fresh and lazy; before Error mounts, faults answer nil.

Methods are installed with ordinary `method` calls on the existing canonical
behavior. Replacements remain observable through fresh lookup, and subsequent
Prelude loading must not reinstall or overwrite Float methods. Canonical Float
roots, storage layout, collection checks and constructor lifetime stay in Id;
the host retains the new library for the same lifetime as other native rungs.

No aggregate facade or new interface design is needed for this checkpoint. Its
boundary is the already established native module protocol. Production host and
installer inventories gain the library; direct native fixtures must load it
when they need Float behavior. The current Rust host remains the loading host,
not a completed platform-only siphon.

## Validation

Focused debug checks pass:

- The independent native fixture loads Id, the tracer/coordinator and Integer,
  then verifies Float arithmetic before Prelude or Error loads. After mounting
  Error it checks 676 arithmetic cases against C double arithmetic, including
  NaN, infinities, signed zero and subnormals. It also checks ordering, all four
  Integer conversions, sqrt/domain errors, fresh boxes, lazy Error propagation,
  Float-only equality, wrong receiver/arity, printing and GC survival.
- A copied IdApi with a replaceable constructor proves the methods consult the
  current constructor. Replacing `+` before loading Prelude proves the later
  module preserves that binding. Restoring it restores normal mixed arithmetic.
  The same native fixture passes against the published debug prefix.
- All 21 Float method/helper bodies match their prior Prelude bodies except
  whitespace. Float Clippy passes with warnings denied; host tests pass 86 with
  one ignored, and both Prelude tests pass. The Float dylib matches its three
  exports and relocatable install name, links only the system library and has
  only the no-state ABI crate as a normal Rust dependency.
- The debug prefix contains 21 native libraries and 28 compiled Birnam modules.
  The Float image probe passes ordinary startup, a compiled module after its
  source file is removed, a saved v8 image containing native Code, and a baked
  executable. The image forces GC around arithmetic and live `+` replacement,
  checks conversion, Float-only equality and alias identity, and confirms the
  Float library is dynamically loaded in startup and both image modes.

Run `mise run float` and `scripts/float-image-smoke.sh PREFIX`. Local evidence is
in `.cargo-target/81q-{native,clippy,unit,prefix,image}.log`. No release validation
or performance claim is made.

## Next checkpoint

Move canonical Float objects, its eight-byte representation, constructor,
accessor and exact recognizer into this module; migrate all production consumers
and remove their four IdApi fields. Keep shared image/recovery payload handling
behind Float's owned codec, following the Integer precedent.

Integer's mixed operations consume Float while Float's mixed operations and
conversions consume Integer. Construction must therefore distinguish publishing
canonical families from attaching their complete numeric dependencies. Resolve
that startup cycle explicitly before removing the floor fields; do not hide it
behind a compatibility facade or move ordinary numeric semantics into Id.
