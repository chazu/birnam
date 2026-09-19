# 81AN — Word uses shared Error value policy

> **Status:** Implemented, following shared Prelude error policy in 81AM.

Remove Word's identical make_error/numeric_error copies and import Error Value's
ABI adapter at the existing asInteger overflow call. Keep that method and every
other Word body unchanged. Preserve exact recognition, live payload reading,
the unsigned boundary, fresh Integer construction on success, and lazy Error
dispatch on overflow. No Word representation, public method or API version change.

Move the existing Error Value provider to immediately before Word activation in
the native module sequence. String is already prepared before Integer; Word's
storage preparation stays there too. Error Value creates no objects, methods or
roots and does not retain an execution table. Attach its dependency before Word's
module_init can publish or register methods. Word's existing prepared instance
continues to select the execution table supplied to the shared helper.

Check independent traces against installed 81AM for absent/null/replaced Error,
current nil, live kind/message construction, null/alias factory results, ordinary
and custom lookup, reentry, GC and calls during activation. Keep the existing Word
storage/preparation checks and verify fresh external debug startup, source-free
compiled module, saved v8 native Code image and baked executable. Update fixtures'
explicit build closures for the new dependency. Commit before moving to another
consumer. No release validation or new language functionality.

## Validation

- The removed helpers match Error Value's existing policy. Every remaining Word
  function body and its storage implementation are unchanged. Startup changes
  only the existing provider's position, from before Prelude to before Word
  activation. Word has no forwarding helper or duplicated Error construction.
- An independent native oracle passes 24 boundary/construction combinations,
  with complete callback traces matching installed 81AM, development libraries
  and the fresh installed prefix. It includes zero, INT64_MAX, its unsigned
  successor and UINT64_MAX, plus null/alias kind, message and factory results.
- Registration calls the supplied asInteger Method on an overflowing Word before
  returning. The dependency is already usable and preserves the original
  execution callbacks. Invalid arity/receivers skip error construction; absent
  Error still interns the kind and creates the exact message before checking the
  export. Null imports return current nil; negative presence flags remain true.
- Kind interning can replace String's constructor and context. Message creation
  can publish a factory before lookup, while a later selector callback preserves
  the already imported factory snapshot. Ordinary method replacement, effectful
  custom lookup, nested overflow, callback GC, copied Methods after replacement
  and reinitialization preserve their prior results and canonical identities.
- Word's seven-symbol inventory adds only Error Value attachment. The Word v1
  descriptor/codec, install name and system-only linkage are unchanged; Prelude's
  exports are unchanged. Both development and installed artifacts match the
  updated inventory. Explicit fixture build commands that prepare Word now
  include Error Value, as required by their shared dependency attachment helper.
- Focused Word checks (81 equality cases plus storage, conversion, preparation,
  publication and GC checks), Error Value (114), System Output (680 in each of
  three modes), Text View (682), System GC (496 in each of three modes), Printing
  (1,104 in each of two modes), Boolean (34) and Float (676 arithmetic cases) pass,
  including installed native checks. All 145 stdlib assertions pass. Word's
  storage unit test, host tests (86, one ignored), builder tests (four), and Word
  Clippy with warnings denied pass.
- The fresh external debug prefix still contains 36 native libraries and 28
  compiled modules. All 1,640 source hashes stayed unchanged during installation;
  this validation record was added afterward. Installed package metadata and
  all 64 required libraries match the install-smoke inventory. Neither release
  validation nor the full install-smoke script was run.
- Installed 81AM and the new prefix pass dynamic startup, a source-free compiled
  module, a saved v8 native Code image and a baked executable. A temporary native
  fixture uses Word v1 to create INT64_MAX and UINT64_MAX values. The image saves
  unexported clones: exported fixture values would instead be external references
  by name under the existing image contract. The helper is removed before image
  boot, which proves the saved Word values survive independently. Checks cover
  exact bits, boundary conversion, aliases, precise GC and fresh Error factory
  replacements returning an object alias, a String and nil.

Receipts are `.cargo-target/81an-{check,build,compare,word-error-value,word,
error-value,system-output,text-view,system-gc,printing,boolean,float,unit,clippy,
prefix,prefix-native,image-baseline,image,stdlib}.log`, plus source hashes and
baseline/development/installed overflow traces.

## Next boundary

Migrate Float's identical recognition and construction helpers onto Error Value.
Float preparation also does not use error policy; move the provider before Float
activation while retaining the earlier String/Word/Float storage preparation.
Preserve arithmetic/comparison error propagation, division/domain/overflow
messages, current callback selection and all numeric results. Integer's earlier
activation boundary follows as a separate migration.
