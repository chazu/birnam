# Alien over Id

Bootstrap loads Alien after TCC and the compiler. Host supplies the installed
library directory. Alien owns library handles and ForeignAddress buffers;
all language values, method publication, roots and compilation use owner messages.
Signature parsing, ABI classification and wrapper generation belong to
[AlienCompiler](../../modules/alien-compiler.birnam), invoked through
`compile:signature:`. The receiver-first C convention remains: the first C argument comes from the Birnam receiver.

Generated wrappers are current native modules compiled by `TCC compile:`.
Initialization returns a declared Method retaining its TCC Unit. The current
Method dictionary publishes it. Callbacks retain their Block and Unit until `releaseCallback:`;
release clears the returned address. Each invocation roots intermediate values,
copies C strings and retains its Unit, including during nested callbacks and
collection. C callbacks must run on the creating thread and complete synchronously;
a native protocol failure aborts at that foreign boundary. String callback returns
remain unsupported because there is no C-side ownership contract for them.

`alloc:` returns zeroed, owned memory. `free` clears its address; collection frees
unreleased owned buffers. Raw peek/poke operations retain the existing caller-owned
C memory bounds contract. Borrowed pointer values never free their pointees.
`dlclose:` refuses while any wrapper Unit remains live, matching the existing
conservative library policy; collecting obsolete Methods releases that restriction.
The authored `AlienRecipes` owner keeps ordinary Maps for function/signature
object bytes and target/selector registrations. Registration stamps come from Id;
they do not retain native Methods or executable Units. Snapshots consult current
bindings, omit removed/replaced wrappers and discard process-local index keys.

`installRecipe:` maps saved object bytes, resolves the destination process address,
publishes through the current Method dictionary and returns the constructed Method.
It performs no cache recording. Image retains those Methods only for its current
operation, repairs Maps after all foreign bindings exist, and then asks the authored
owner to index registrations. Independently edited cache bytes remain unchanged.
`bind:signature:as:on:` declares a prepared binding for artifact compilation.
Codegen embeds its object bytes and emits the live
`bind:signature:as:on:code:` send. That entry resolves the process address and
uses the same Method publication and `AlienRecipes` recording as ordinary wrap.
Uncompiled source uses the normal wrapping path; lexical receiver shadowing is
not transformed. Ordinary `wrap:signature:as:on:` uses the current authored cache and recording methods;
no Store service or mirror is involved. Callback addresses and foreign memory remain
process-local. Error predicates remain current messages, including private Error
behavior.

`provideLibrary:named:` stages embedded bytes in an owner-private directory,
without opening them. `provideLibraryPath:named:` binds an artifact's declared
native sibling, without changing the fallback `libraryDir:`. Both use bare logical
names and reject conflicts with different open libraries. `dlopen:` then resolves
these declared names before using the ordinary fallback directory. All embedded
siblings are supplied before source activation. Private files are removed at
registry Drop or DSO teardown; no Id callback survives into teardown. Existing
close/refcount and live-wrapper restrictions are retained.

`mise run native-messages` includes the production Alien test, covering actual
scalar, unsigned, aggregate and callback calls, unaligned buffers, collecting
conversion replacements, recursive callbacks and final TCC reclamation.
`cargo test --offline --locked -p birnam-alien` checks the library registry.
`scripts/alien-compiler-test.py PREFIX` compares the authored compiler with a
frozen test-only Rust reference; that reference is outside the production workspace. The older frozen-IdApi fixtures are not current
acceptance. `mise run host-cli` validates installation and relocated startup.
