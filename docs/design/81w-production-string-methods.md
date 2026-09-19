# 81W — String methods above Id

> **Status:** Implemented, following nil methods in 81V.

Move fourteen existing String methods from Collection into `birnam-string`:
`printString`, `,`, `=`, `size`, `at:`, `byteAt:`, `copyFrom:to:`, `trimmed`,
`indexOf:`, `bracketDepth`, `replace:with:`, `asSymbol`, `asInteger`, and
`asNumber`. Load it immediately before Collection, after nil. Use current Method,
module-init and IdApi interfaces, with the existing explicit Integer and Float
dependencies. Pin the library for installed methods and native roots.

Copy the method bodies unchanged. Preserve byte indexing and exclusive slice
ends, exact String recognition, fresh copied results, arbitrary embedded bytes,
NUL-terminated Symbol conversion, Integer/Float conversion selection and bounds,
and the different whitespace sets used by trimming and number parsing. Printing
returns its receiver without an identity check. Concatenation sends the cached
`printString` selector through the current send callback and performs fresh
method lookup; an invalid printed result produces a fresh copy of the left side.
Preserve every callback, temporary copy and error/nil return path.

The printing selector moves into String's own native state and root provider.
Register that provider before interning it; mark only after the native state is
initialized. Do not depend on Collection or the continued presence of a method
key to retain the selector. Repeated initialization retains native state and
reinstalls the methods, while later Collection must preserve replacements.

Keep `split:` and its private byte-copy helper in Collection: splitting still
uses native Array construction/push and must not become ordinary Array sends.
Keep Collection's String parent association and StringBuilder unchanged. Id
continues to own String identity, raw payload, external buffers, borrowing and
cleanup; this checkpoint changes no representation, IdApi or image format.

Review confirms that the fourteen methods do not use Collection payloads or
state beyond the printing selector. The byte-copy helper is duplicated narrowly
for the retained splitting method; it introduces no shared native interface.
Compare all moved method/helper bodies and installation calls against committed
source. Validate against an independent C oracle and prior Collection behavior,
including binary bytes, indexes/slices, text and numeric boundaries, inherited
recognition, live callback/constructor replacement, selector rooting, repeated
initialization and later Collection. Finish with focused debug startup,
source-free compiled module, saved native-code image and baked executable checks,
then commit. No release validation or new functionality.

## Validation

The independent C oracle passes against both the prior Collection and the new
String module: 410 binary/index/slice/combination cases and 18 numeric boundaries,
plus trimming, search/replacement, Symbol conversion, bracket depth, malformed
arguments, exact versus inherited recognition, live byte/String/Integer/Float
callbacks, fresh concatenation lookup, collection in a printing callback,
selector roots, namespace rebinding and repeated initialization. The new module
preserves replacements through later Collection, and retained native splitting
still constructs the expected Array contents. All fourteen method bodies, two
byte/text helpers and installation calls match the prior Collection byte-for-byte;
the retained splitting method is also unchanged.

Collection's four tests pass, as do 86 host tests (one ignored) and four builder
tests. String Clippy passes with warnings denied. Development and installed
dylibs match the four declared exports and relocatable install name, linking
only the system library. IdApi, Id representation and Image sources are unchanged.

A fresh external debug prefix contains 25 native libraries and 28 compiled
Birnam modules. All 1,485 source hashes remain unchanged during installation.
The installed native fixture, startup, a compiled module after source removal,
a saved v8 image containing native Code and a baked executable pass. Both image
modes exercise byte slicing, numeric conversion, captured String alias identity,
Array splitting, live method replacement and collection, and confirm dynamic
String loading.

Run `mise run string` and `scripts/string-image-smoke.sh PREFIX`. Local logs are
`.cargo-target/81w-{check,baseline,native,unit,clippy,prefix,prefix-native,image}.log`.
No release validation ran.

## Next boundary

Audit String representation and external byte ownership before removing its
three IdApi fields. Unlike the numeric payloads, Id currently owns a private
object-to-buffer map: `stringBytes` reads that owner rather than trusting raw
payload words, and clone copies the owned bytes and finalizer metadata. Ordinary
construction, internal construction, clone and recovery must keep their distinct
callback/root schedules. The collector and deferred-allocation optimizer also
recognize one exact nonobserving cleanup callback and its owning runtime; that
proof must move with the byte owner rather than becoming a permissive finalizer
classification. Preserve per-runtime isolation, overwritten finalizers, stable
byte borrowing, inherited Image recognition and the frozen image encoding.

StringBuilder remains a separate native-buffer family in Collection. Its
construction, finalization, reinitialization/publication and image limitations
need their own review; it should not be folded into String's immutable byte owner.
