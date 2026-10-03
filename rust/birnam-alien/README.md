# Alien over Id

Bootstrap loads Alien after TCC and the compiler. Host supplies the installed
library directory. Alien owns library handles and ForeignAddress buffers;
all language values, method publication, roots and compilation use owner messages.
Signature parsing, ABI classification and wrapper generation belong to
[AlienCompiler](../birnam-alien-compiler/README.md), invoked through
`compile:signature:`. The receiver-first C convention remains: the first C argument comes from the Birnam receiver.

Generated wrappers are current native modules compiled by `TCC compile:`.
Initialization returns an Execution Method retaining its TCC Unit. Store publishes
the Method. Callbacks retain their Block and Unit until `releaseCallback:`;
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
Store retains `__aliens__` recipes with function/signature, selector, target and
relocatable object bytes. `restoreWrapper:` maps those objects and binds the
current process address without compiling. Repeating a wrap reuses recorded
object bytes. Callback addresses and foreign memory remain process-local.

`mise run native-messages` includes the production Alien test, covering actual
scalar, unsigned, aggregate and callback calls, unaligned buffers, collecting
conversion replacements, recursive callbacks and final TCC reclamation.
`cargo test --offline --locked -p birnam-alien-compiler -p birnam-alien` checks
the parser, classifier, C emission and library registry. The older frozen-IdApi fixtures are not current
acceptance. `mise run host-cli` validates installation and relocated startup.
