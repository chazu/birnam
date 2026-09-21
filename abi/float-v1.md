# Native Float dependency v1

`birnam-float` owns Float's canonical behavior, zero prototype and eight-byte
IEEE payload. Its production entry constructs Float after Integer and before
Word, Block or Runtime. The still-combined Machine and image consumers acquire
the completed family through `birnam_float_dependency_v1`.

`FloatApiV1` is a 72-byte C table: version, byte size, stable native context,
behavior and prototype identities, constructor, reader, exact recognizer and
pointer to an immutable 48-byte `FloatStorageV1`. The codec declares payload
size/alignment and bit-preserving initialization/read callbacks. See the
[independent C declaration](fixtures/float-v1/float_v1.h).

## Production construction

`birnam_native_module_v1` receives the real Id protocol and seven operands:
Words pointing to completed root-registry, Lifetime, base-values, Namespace and
String descriptors, the Error factory as an Object, then a Word pointing to the
completed Integer table. Object Model is already installed. The entry returns
a Word pointing to Float's existing operations table. No new representation or
ABI version is introduced.

Float creates its behavior/prototype through current Id messages, records its
raw layout in Lifetime, associates its owner and publishes Float/FloatVt.
Its own root provider retains these identities, predecessor references and
cached selectors, including during construction. `fromBits:` accepts a native
Word carrying all 64 payload bits; `bits` returns those bits. Neither requires
the language Word module. The seventeen existing methods are installed at this
stage: arithmetic, exact Float equality, mixed comparisons, negation, absolute
value, Integer conversion, floor/ceiling/rounding, square root and printing.

Division by either signed zero reports `zeroDivide`; negative square roots
report `domain`; non-finite/out-of-range Integer conversions report `overflow`.
Error recognition reads the current Namespace ErrorVt binding. Construction uses
current String and Error factory messages. Existing NaN, infinity, signed-zero,
rounding and printing behavior is preserved.

After Float is complete, its initializer sends Integer `installFloat:` with the
completed Float table as a native Word. That explicit extension entry creates a
separate context containing the completed Integer and Float owners, then installs
mixed arithmetic/comparisons and `asFloat` on Integer. It replaces only the
original base entries and preserves earlier method replacements. Integer's base
has no absent Float dependency or late-filled slot. `to:do:` still waits for
Execution/Block; the later legacy mount supplies that remaining extension.

## Allocation, adoption and lifetime

Consumers copy the current operation table before calls. Production construction
sends the current Id `allocate` method and checks actual allocation size/behavior
before writing payload bytes. Exact recognition checks registered storage and
the canonical behavior. The unsafe reader remains valid for a live Float payload
after behavior reassociation. Pure codec callbacks neither allocate nor dispatch;
image materialization retains its existing allocation/root schedule. Float has
no native numeric optimization declaration or allocator-context guard.

The host skips legacy Float preparation and attaches the same completed table to
upper consumers. Late `module_init` checks that it is mounting on the same Id and
preserves native identities, layouts, bindings and method contexts. Rooted early
objects survive Runtime adoption and precise GC; releasing their last roots
permits reclamation. Native libraries and contexts remain pinned after entry,
including if initialization subsequently fails.

The legacy `birnam_float_prepare_v6` and IdApi `module_init` remain for existing
native clients pending their migration. That path prepares Float before Integer,
then publishes its names and installs methods after Integer/String/Error Value
attachment. It retains its selected execution capabilities and original repeated
method-installation behavior. It is not the production constructor. Legacy
consumers must use the current IdApi; historical manifests are evidence only.

The [eight-symbol inventory](rust-float-dylib-v1.json) includes the native entry.
Run `scripts/id-native-loading-smoke.py PREFIX` for the real construction prefix,
independent IEEE/conversion checks and adoption/GC; `scripts/float-image-smoke.sh PREFIX`
and `scripts/float-error-value-image-smoke.sh PREFIX` cover source, compiled,
saved-image and baked execution. `mise run float` exercises legacy native clients
using a debug build. The remaining Design 83 stages and compiler admission are
tracked in the [design document](../docs/design/83-production-id-construction-and-lifetimes.md).
