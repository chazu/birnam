# 81Y — StringBuilder above Collection

> **Status:** Implemented, following String representation in 81X.

Extract StringBuilder's existing native buffer family and seven selectors
(five Method implementations) from Collection into `birnam-string-builder`.
It inherits directly from Object and has no collection payload dependency.
Load it immediately before Collection, after String. Use existing IdApi v7,
Integer and String dependencies; add no family capability or image encoding.

Keep the three-word pointer/length/capacity payload, RAW layout, geometric growth
from 64 bytes, byte truncation, mutation/identity returns, fresh String copies and
exact String argument recognition. Conversion sends the cached printString
selector through the current send callback. New and clone both create an empty
builder, regardless of receiver; nonzero arity answers nil. Copy every method
and allocation/growth/finalization helper unchanged. In particular, retain the
ordinary replaceable alloc and setFinalizer calls, unlike immutable String's
native allocation capability. Cleanup frees the external buffer and zeroes all
three words at its existing sweep position.

Initialization independently adopts existing StringBuilder/StringBuilderVt
exports or creates a family; it caches the selected objects and printString
selector. Reinitialization reapplies layout and reinstalls its seven methods.
Preserve the actual existing publication predicate: export owner/behavior/prototype
only when **Vector** is not exported. This is an existing cross-family rule,
not an Array predicate or a new builder-only guard. Loading before Collection
preserves the ordinary startup case and the existing Vector-present/adopted-family
cases. Publication order within the new module remains owner, StringBuilderVt,
StringBuilder. Native reflection follows module construction/publication order,
as in earlier extractions. Later Collection initialization leaves this family's
methods and layout to its new owning module.

Give the module its own root provider and mark partially initialized identities
before subsequent foreign callbacks; retain all three cached Oops after namespace
rebinding. Pin its library for the installed methods/finalizer/root provider.
Remove every Builder payload, method, root, layout and publication from Collection.
Do not retain an aggregate compatibility facade.

Image's generic RAW handling currently copies a builder's native address and
length/capacity words without reconstructing its external buffer or finalizer.
A captured live builder is not a portable image value. Preserve this limitation:
no new serialization/rejection policy in this extraction. Validate compiled
images whose entry constructs and uses builders, and captures their resulting
Strings; record a baseline raw-capture probe without dereferencing a restored
native address. Keep Image source and the frozen encoding authority unchanged.

Compare the copied bodies against committed Collection source. Validate with an
independent C byte oracle against the previous Collection and the new module:
growth boundaries, arbitrary bytes, malformed arguments, empty clone/new, live
String/Integer/Id callbacks, conversion reentry/GC, cleanup/reset, namespace
rebinding, adoption, Vector-present publication and repeated initialization.
Finish focused debug startup, source-free compiled module, saved native-code
image and baked executable checks, then commit. No release validation or new
language behavior.

## Validation

- All eleven allocation, growth, cleanup and Method bodies, plus seven selector
  registrations, match committed Collection source modulo formatting. Collection
  loses 301 lines and every Builder responsibility. Id, IdApi and Image source are
  unchanged.
- The independent C oracle passes against the previous installed Collection and
  both development and installed StringBuilder libraries. Each runs four fresh
  processes: ordinary construction, adopted Builder, Vector already present,
  and both adopted Builder and Vector present. Each checks 21 byte/growth cases
  plus callback replacement, reentry/GC, exact recognition, inherited receivers,
  empty clone/new, forced native cleanup, retained roots and reload publication.
  New-module preparation additionally tolerates collection during allocation
  and selector interning; publication reentry constructs and collects builders.
- Collection's four tests, host's 86 tests (one ignored), and install driver's
  four tests pass. StringBuilder Clippy passes with warnings denied.
- Installed String's 410 byte/index/slice cases and 18 numeric boundaries, and
  nil's 30 receiver/method cases, pass after Collection's Builder removal.
- The installed stdlib suite passes all 145 assertions, using its normal
  ConsoleRunner/OS exit convention.
- Development and installed StringBuilder match the four-symbol export inventory,
  relocatable install name and system-only linkage.
- A fresh external debug prefix contains 26 native libraries and 28 compiled
  modules. All 1,507 source hashes remained unchanged throughout installation.
  Subsequent edits only document results and update install-smoke's expected
  package/library inventory.
- Ordinary startup, a compiled module with its source removed, a saved v8 image
  containing a native Code record, and a baked executable all pass. The latter
  two preserve captured materialized String aliases, construct fresh builders,
  grow/copy buffers, truncate byte inputs, use empty clone/new, exercise stdlib
  conveniences, replace a live method and collect. Dynamic library loading is
  observed. The same oracle passes against the pre-extraction installed build.
- Both builds preserve the recorded live-buffer capture limitation: saving a
  captured nonempty builder emits a native pointer, length and capacity as three
  RAW words. The probe examines the bytes without executing that image.

Focused logs are `.cargo-target/81y-{check,baseline,native,unit,clippy,prefix,
prefix-native,baseline-image,image,stdlib}.log`. No release validation was run.

## Next boundary

Range is the smallest remaining family, but it has two direct Vector consumers:
`reversed` and `,` construct persistent vectors through private helpers. Map's
`keys`/`values` and String's `split:` share those helpers too. Range, Vector and
Map also share Collection identity and the Vector-guarded initialization branch.
Audit that construction and storage boundary before moving another whole family;
do not replace direct helper calls with overridable sends or add an aggregate
Collection forwarding facade. A methods-first Range checkpoint can isolate its
six non-Vector methods while preserving the existing representation and keeping
those two vector-producing methods with their storage owner.
