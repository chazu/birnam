# 81S — Word methods above Id

> **Status:** Implemented, following Float representation in 81R.

Move the existing `printString`, `=` and `asInteger` methods from Prelude into
`birnam-word`, loaded after Float and before object-model/Artifact. The module
depends only on the no-state ABI crate and explicitly receives Integer for
conversion. The existing Method and module-init interfaces suffice; no new
interface design or pointer abstraction is introduced.

Keep the method bodies unchanged. Printing uses lowercase hexadecimal prefixed
by `0x`, equality compares only exact Word values, and `asInteger` rejects values
above signed-64 maximum through the same lazy Error factory. Wrong receivers or
arity answer nil; invalid equality operands answer false. Reads use the current
IdApi operations and conversion uses the current Integer operations. Later
Prelude initialization must preserve a replacement Word method.

Word's canonical objects, raw payload, constructor, reader, recognizer and roots
remain in Id at this checkpoint. IdApi v5, image bytes, Method and Frame contracts
stay unchanged. The module's native code remains pinned for installed methods.
No Alien or ForeignAddress behavior moves here.

Validate against an independent native boundary-value oracle, including lazy
overflow errors, live accessors/construction and method replacement. Check debug
startup, a source-free compiled module, a saved native-code image and a baked
executable, then commit. Representation and its actual consumers are the next
checkpoint, requiring their own caller and lifetime audit.

## Validation

The independent C fixture passes 81 equality pairs, all nine unsigned boundary
values for printing/conversion, lazy overflow before Error loads, actual Error
kind afterward, wrong receiver/arity, fresh Integer boxes, current Word accessor
and Integer constructor wrappers, method replacement before/after Prelude, and
precise collection. It passes both against development dylibs and the installed
debug prefix. All eight moved method/helper bodies match the previous Prelude
bodies apart from whitespace.

Host tests pass 86 with one ignored; its builder passes four tests and Prelude
passes two. Word Clippy passes with warnings denied. Development and installed
Word dylibs match the three declared exports and relocatable install name; the
module links only the system library and depends only on the ABI crate.

The external debug prefix contains 22 native libraries and 28 compiled Birnam
modules. Source hashes stayed unchanged during installation. The Word probe
passes startup, a compiled module after removing its source, a saved v8 image
with native Code and a baked executable. Both image modes exercise printing,
Integer conversion, alias identity, Word-only equality, live method replacement
and collection, and confirm that the Word dylib was dynamically loaded.

Run `mise run word` and `scripts/word-image-smoke.sh PREFIX`. Local logs are
`.cargo-target/81s-{check,unit,clippy,native,prefix,prefix-native,image}.log`.
No release validation was run.

## Representation follow-up

The initial caller inventory finds actual Word representation use in Id,
TCC, Image and the new Word module. Preserve TCC's generated-code entry names
and its unsigned-value recognition, Image's inherited-family recognition, and
Machine's boxed frame-identity values. ForeignAddress already owns its own
family; this extraction must not merge it with Word. Review these consumers and
bootstrap timing before removing `word_vt`, `mkWord`, `wordValue` and `isWord`.

[81T](81t-production-word-storage.md) completes this representation follow-up.
