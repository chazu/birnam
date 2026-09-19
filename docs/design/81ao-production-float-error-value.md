# 81AO — Float uses shared Error value policy

> **Status:** Implemented, following Word's migration in 81AN.

Remove Float's matching is_error, make_error and numeric_error copies. Import
Error Value's adapters at the existing calls, keeping every numeric method,
comparison, conversion, representation and preparation body unchanged. Preserve
exact Error propagation before numeric right-operand conversion, division's
zero check, and the existing type/domain/overflow messages and results. Equality
does not use Error recognition and must remain distinct from ordered comparison.

Move the existing Error Value provider before Float activation, after Integer.
String and Float storage are already prepared before Integer initializes. Keep
that preparation order; attach Error Value before Float's module_init can publish
or register methods. Float's prepared instance keeps its original execution
table, passed to the stateless shared policy on every call. No API version or
layout change, extra roots, eager Error loading or new functionality.

Compare independent native traces against installed 81AN for every affected
method, including calls during registration, exact/inherited recognition,
absent/null/aliased ErrorVt, Error precedence over numeric operands, lazy factory
construction, current nil, null/alias results, fresh/custom lookup, callback
mutation, reentry, GC and reload. Retain the existing IEEE/numeric oracle. Verify
a fresh external debug prefix, source-free compiled module, saved v8 native Code
image and baked executable before committing. No release validation.

## Validation

- The three removed helpers match Error Value's existing policy. Every remaining
  Float function body and the storage implementation are unchanged. Startup's
  only change is the provider's position, from before Word to before Float.
- An independent native oracle passes 373 failure, recognition and construction
  combinations. Complete callback traces match installed 81AN, development
  libraries and the fresh installed prefix. All thirteen affected methods are
  also invoked while their registrations are in progress.
- The oracle preserves receiver reads before rejection of a null binary argument
  carrier, exact Error recognition before numeric conversion or zero division,
  rejection of inherited Error behavior, and equality's separate numeric rule.
  It covers absent/null/aliased ErrorVt, canonical Float/Integer aliases, both
  signed zero divisors, nonfinite and out-of-range conversions, and every
  existing failure kind and message.
- Error construction preserves current nil, null/aliased kinds, messages and
  results, String constructor/context replacement during kind interning, factory
  publication during message creation, and the imported factory snapshot across
  selector interning. Fresh ordinary/custom lookup, nested construction, callback
  GC, copied Methods, precise collection and reinitialization preserve results,
  canonical identities and the original selected execution table.
- Float's seven-symbol inventory adds only Error Value attachment. The v1
  descriptor and storage codec, relocatable install name and system-only linkage
  are unchanged. Development and installed libraries match the inventory; Word
  and Prelude exports are unchanged. Existing explicit Float fixture build
  closures already include Error Value.
- Focused Float checks (676 IEEE arithmetic cases plus ordering, conversion,
  storage, preparation and GC checks), Word Error Value (24), Word (81 equality
  cases plus storage checks), Error Value (114), System Output (680 in each of
  three modes), Text View (682), System GC (496 in each of three modes), Printing
  (1,104 in each of two modes) and Boolean (34) pass, including installed native
  checks. All 145 stdlib assertions pass. Float's storage unit test, host tests
  (86, one ignored), builder tests (four), and Float Clippy with warnings denied
  pass.
- The fresh external debug prefix contains the same 36 native libraries and
  28 compiled modules. All 1,644 source hashes stayed unchanged during installation;
  this validation record was added afterward. Installed package metadata and all
  64 required libraries match the install-smoke inventory. No release validation
  or full install-smoke run was performed.
- Installed 81AN and the new prefix pass dynamic startup, a source-free compiled
  module, a saved v8 native Code image and a baked executable. The checks preserve
  Float values and aliases, arithmetic, exact Error propagation, failure kinds,
  precise GC and fresh Error factory replacements returning an object alias, a
  String and nil. The overflow value uses a decimal Float literal at 2^63;
  exponent notation is not supported as a Float literal by the baseline parser.

Receipts are `.cargo-target/81ao-{check,build,compare,float-error-value,float,
word-error-value,word,error-value,system-output,text-view,system-gc,printing,
boolean,unit,clippy,prefix,prefix-native,image-baseline,image,stdlib}.log`, plus
source hashes and baseline/development/installed Float error traces.

## Next boundary

Migrate Integer's remaining identical recognition and construction helpers onto
Error Value. Preserve arithmetic, division/modulus, ordered comparisons and
`to:do:` block-result propagation, along with the native numeric declaration.
Move the provider before Integer activation while retaining the order of String,
Word and Float storage preparation. The host currently triggers preparation when
it reaches Integer, so that trigger must move with the earlier provider boundary;
changing only the module list would leave String unavailable. Update independent
fixture mounting to attach Error Value before Integer initialization and exercise
error paths during method registration. Keep Integer's original execution table,
canonical identities, numeric admission and callback order unchanged.
