# Ordinary slot reflection and the first Module field migration

2026-10-14. Implements the next bounded slice of
[optional Store integration](optional-store-and-relational.md).

## Contract

Reflect remains an authored facade over the storage owner. New public messages:

| Message | Meaning |
| --- | --- |
| `Reflect slotsOf: object` | Snapshot of discoverable ordinary **reference** slots, in index order; nil for unsupported storage. Does not read their values. |
| `Reflect slot: key in: object` | Read the current reference through ObjectModel. Key is a nonnegative Integer index or a Symbol name. |
| `Reflect slot: key in: object put: value` | Write through ObjectModel; answers the object, as its existing slot writer does. |

Each inventory row is a Map with `#slot` (Integer index), `#name` (Symbol or nil),
`#owner` (ObjectModel), `#readable` and `#writable` (both true for the reference
slots supported in this slice). These are structural capabilities, not promises
that a later write will pass current bounds/layout or GC-phase admission.
The index is stable within an object's existing storage, not a globally unique
identity or a lease on a future layout. Old rows are snapshots, not live views.

Objects describe optional names through the ordinary `slotNames` method, a Vector
indexed by physical reference-slot position. Object's default is empty, so unnamed
slots remain discoverable by index. Cell describes `#value`; Module describes
`#libDir`. This is imageable, replaceable Birnam metadata, not a new native schema
registry. Symbol keys resolve through the current names on each access; duplicate
names follow Vector's first-match convention. Numeric indices do not depend on
names. Renaming a field does not move its contents.

ObjectModel's new `referenceSlotsOf:` query reads only its own allocation/layout
metadata and returns a snapshot of Integer indices. It follows private behavior
layers to their nearest layout, as ordinary image transport already does. It
never decodes foreign payloads or returns raw-word fields as references. Empty
ordinary storage answers an empty Vector; unowned nonempty storage or missing
layout answers nil. Owned raw-only storage (such as an Integer allocation)
answers an empty reference inventory; its raw contents are not decoded.
Native families still need their own logical inspection
protocols; this is not a universal native-payload inspector.

Reflect rejects unsupported storage with `#unsupportedSlots`, and an absent name,
invalid index or non-reference slot with `#unknownSlot`. It resolves membership
against the current owner inventory before access, then uses the existing owner
read/write sends, which enforce their own bounds/layout/lifetime contracts.
Owner and name-provider errors propagate. Inventory rows retain the owner used
for that query, even if a naming callback changes a global binding.
Inventory construction, naming, resolution and access remain replaceable. No
cached method, schema proof, monkeypatch guard or compiler specialization is added.
The authored facade is resumable; replacing its owner query or naming method can
wait and resume. Primitive owner's synchronous construction callbacks retain the
same native callback boundary as its existing slot/constructor primitives.

Inspection and edits create no Store facts or history. `Reflect factsOf:` keeps
its separate meaning: deliberately retained database facts.

## Bounded Module migration

Only Module's `libDir` field moves in this slice. The Module object has one
ordinary reference slot initialized to nil. `libDir` retains the existing `"lib"`
default for nil; `libDir:` retains its return value and stores through ObjectModel.
Existing host rebasing still sends `libDir:` at startup and image restoration.
The slot persists through the ordinary-object image machinery. Clones initially
share its value but own independent slots. This deliberately adopts ordinary
shallow-clone behavior: the previous external per-entity fact was not copied by
Object clone, so a fresh clone formerly began with the default directory.

Generic inspection and mutation are now `Reflect slotsOf: Module` and
`Reflect slot: #libDir in: Module` / `Reflect slot: #libDir in: Module put: path`.
The latter exposes the actual stored value, including nil, while Module's getter
continues applying its default. The old `db` fact/Cell for Module/#libDir is no
longer a live alias for new Module objects. Explicit facts at that key remain
ordinary independent Store data; they are not silently forwarded, deleted or
mirrored. Tools requiring the live field must use Module or Reflect. This is the
intentional public storage/API transition, not a claim to preserve arbitrary
queries or writes to the former fact representation.

Old images keep their saved Module object and old methods, hence their fact-backed
libDir behavior; they are not reshaped or upgraded during load. New images keep
the ordinary slot. Both must still rebase to the current prefix. The registration,
kind, dependency and loaded-state facts remain unchanged, as do traversal, artifact
admission and idempotent loading. Full registry migration is a later slice.

This does not remove Store from normal bootstrap: Globals, publication, metadata
and the remaining registry still depend on it. Store/Relational self-description
remains intact. Inspection and basic ordinary mutation must also be exercised with
Store absent on the smaller exported-program floor.

## Lifetime correspondence

The native inventory keeps its target, each observed behavior layer, language
constructors and intermediate results in ObjectModel's nested root inventory. A
metadata callback may detach a behavior and collect before returning its parent;
protecting only the target is insufficient. The adversarial regression reproduced
that missing-root failure in the initial candidate. Explicit behavior-chain roots
fix it while retaining current getter dispatch. Another regression mutates the
layout, reenters enumeration and collects during Integer construction, then checks
snapshot contents, later observations, error cleanup and target reclamation.

## Acceptance and remaining work

[Source-bound evidence](../measurements/reflect-slots-and-module-state-20261014.json).
The preceding Cell/String work was committed as `707f59bd` before this slice.

Passed on the final candidate:

- 780 language assertions, followed by private-scratch, owner reflection,
  code-inspector, Cell and new slot-retention fixtures.
- Four native integration binaries: ordinary ObjectModel semantics, scoped slot
  operations, the new reference inventory, and source/module loading.
- All 17 exported production method bodies on the small floor with Store,
  Relational, Lang, Generation and Bootstrap absent. Namespace supplies bindings
  and existing raw bootstrap publication installs the bodies; this proves basic
  operations, not Store-free activation of the complete library or all Error paths.
- New and legacy Module objects through source-free mapped and rebuilt images,
  including shared aliases, edits after load and current-prefix rebasing.
- Host/CLI receipt and relocation checks on a separate prefix copy, and all 26
  REPL golden checks.
- 237 Lean theorems with standard-axiom auditing, 53 rejected mutations and all
  123 expected TLC outcomes. These are scoped abstract models plus executable
  checks, not a proof of the compiler, collector or entire language.

Across 0, 1, 30 and 100 operation batches, directory/Cell edits and slot inspection
create zero facts and leave the same two-object post-GC delta as the zero-call
control. Explicit database facts remain readable and independent. No CPU speedup
or equal allocation churn is claimed.

The final prefix regenerated native/authored artifacts and startup images using
the development Cargo cache. This is not clean-cache release qualification, and
the user's installed version was not changed.

The next Module slice is an owned registry for registration, kind, dependencies
and loaded state, with discoverable records and supported edits replacing their
public fact paths. Globals, method publication, compiler metadata and coordinated
image/generation ownership remain later steps. The high-level decision remains
unchanged: keep Store/Relational available and self-describing, while removing
mandatory participation from ordinary language state.

Reproduction:

```sh
BIRNAM_TEST_PREFIX=PREFIX bash scripts/birnam-test.sh
python3 scripts/reflect-slots-test.py PREFIX --legacy-prefix PREVIOUS_PREFIX
mise exec -- python3 scripts/cell-without-store-test.py PREFIX --slots
BIRNAM_PREFIX=PREFIX bash scripts/repl-golden.sh
mise exec -- python3 scripts/check-rooting-model.py
```
