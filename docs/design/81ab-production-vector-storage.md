# 81AB — Vector storage and native helpers

> **Status:** Implemented, following Range storage in 81AA.

Move the 32-way Vector trie, five-word payload, canonical Vector/VectorNode
behaviors and prototype, layouts, allocation and pure access helpers into
`birnam-vector`. Move the existing independent Arc-backed kernel and its four
tests with them. Collection keeps its shared parent, Vector Method bodies,
sorting root stack, Map storage, String splitting and the two Vector-backed
Range methods for this checkpoint. Map retains its own radix constants.

An immutable process-pinned native descriptor supplies staged construction and
direct empty/push/count/at/behavior operations. Migrate each actual consumer;
leave no payload copy or helper forwarding facade in Collection. Pure reads
must not call Id, allocate, collect or reenter. Empty/push retain the first
selected Collection IdApi and read its current allocation callback at every
allocation. They preserve the original allocation order, including the copied
tail allocated before the full-tail branch, and canonical result behavior.
No helper becomes an overridable language send; language sends elsewhere keep
their existing live lookup and callback ordering.

Construction stays interleaved with Range and Map. Prepare VectorNode then
VectorVt (then import Vector on adoption), at the original position following
CollectionVt and preceding Range. Return the two behaviors solely for the
existing incomplete-export validation. Finish a missing prototype at the
original point, before Range's prototype. Apply node and payload layouts before
Range's layout. Set the Vector owner before Collection publication, then export
VectorVt, Vector and VectorNode after Collection's two exports. Keep the outer
Vector-absent publication test; do not repeat it inside the publisher.

Adopt nonnull prototypes unchanged, including rebound values; reload retains
the first identities and selected table. Vector owns its three identity roots,
registering before prepare and rooting each acquired identity before the next
foreign callback. Collection retains its own caches and sort temporaries.
Preserve node OOP slots, the RAW/RAW/OOP/OOP/RAW payload, path sharing, indexing,
zero-filled node allocation, ordinary GC ownership and generic image encoding.
Id, IdApi and Image do not change.

Audit construction and operation traces against the committed 81AA binaries,
including cold/adopted/missing/rebound prototypes, namespace rebinding and
reload. An independent native oracle will exercise trie boundaries, persistence,
allocation callbacks, identity roots, malformed dependencies and GC. Verify a
fresh debug install, source-free compiled module, v8 native Code image and baked
executable, then commit this checkpoint. No release validation or new semantics.


## Validation

- The native trie helpers match the committed Collection bodies exactly apart
  from their owning state type. All retained Method bodies differ only in the
  declared native dependency calls. Collection has no Vector payload, layout,
  node helper, constructor helper or cached canonical Vector identity left.
  The independent Arc-backed kernel and its four tests moved with storage.
- The independent C oracle matches 81AA construction and reload traces
  byte-for-byte in five modes: cold, adopted, absent adopted prototype, rebound
  nil prototype, and stale family exports without Vector. The absent-prototype
  case supplies a live `isExported` callback returning true for the initial
  adoption branch, while import returns null. Root-provider registration moves
  to Vector before preparation; the trace compares all existing construction
  operations, deliberately excluding that registration.
- The oracle checks persistence and every index through 32,801 elements,
  retained versions at 32/33 and 1,056/1,057, path sharing, two root-height
  increases, null entries, inherited receivers, empty clone/new, exact node and
  payload layouts, complete payload initialization, no finalizers, canonical
  roots, namespace rebinding and precise GC. Allocation sequences at trie
  boundaries match the prior build, including its initial unused tail copy
  when a full tail moves into the trie.
- Partial and completed Vector roots survive construction callbacks. A changed
  allocator in the original selected table is observed after reload with a
  different table; collection runs inside those allocation callbacks. Native
  return-address inspection places storage allocation calls in Vector.
  Overridden public Vector methods do not intercept Range reversal/concatenation,
  Map projections or String splitting.
- The C descriptor checks reject malformed dependencies before attachment,
  including each missing operation, and reject retargeting after attachment.
  Reattachment to the same descriptor succeeds. Development and installed
  Vector match the three-symbol inventory, relocatable install name and
  system-only linkage. Collection adds only its Vector dependency symbol.
- ABI tests (four), Vector kernel tests (four), host tests (86, one ignored)
  and installer tests (four) pass. Vector and ABI Clippy pass with warnings
  denied. Existing Range tests also pass, including 648 boundary/index cases
  and its five construction modes.
- A fresh external debug prefix has 28 native libraries and 28 compiled modules.
  All 1,534 source hashes stayed unchanged during installation. Installed
  Vector traces match 81AA in every mode; installed Range, StringBuilder's four
  modes, String's 410 byte/index/slice cases and 18 numeric boundaries, nil's
  30 cases, and all 145 stdlib assertions pass.
- The previous and new installed builds pass the same startup, source-free
  compiled-module, saved v8 native Code image and baked-executable scenario.
  It captures an older Vector, a 1,057-element successor and a shared alias;
  checks persistence, indexing, folding, sorting, Range/Map/String consumers,
  stackless iteration with GC, and live method replacement. The new Vector
  library is observed in startup, image and baked execution.

Receipts are `.cargo-target/81ab-{check,range,unit,clippy,build,compare,prefix,
prefix-native,image-baseline,image,stdlib}.log`, with individual baseline,
development and installed native traces. Post-install source edits only improve
fixture admission checks, a smoke-test label and this validation record. No
release validation was run; Id, IdApi v7 and Image source remain unchanged.

## Next boundary

Move the two remaining Range methods into Range using the verified native
Vector storage dependency. Preserve Range concatenation's direct call to the
existing Vector comma implementation through an explicit retained native
callback until Vector method ownership moves too. Do not replace that call with
an overridable comma send. Continue preserving interleaved method registration
and the selected Id table; commit that smaller move separately.
