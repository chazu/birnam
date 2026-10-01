# Lang build seed

This crate retains the previous Rust grammar as an explicit build seed. It is
used only while the installer compiles `modules/modules.birnam` and
`modules/lang.birnam`. A temporary Bootstrap built with `lang-seed` loads
`libbirnam_lang_seed.dylib`; neither is published as the production bootstrap.

Normal Bootstrap loads the compiled Module and Lang artifacts after PEG, TCC and
Codegen are available. It never loads this seed or parses Lang source. The
installer replaces the seed Bootstrap and removes the seed library before
publishing its receipt. The remaining Rust compiler and PEG matcher are separate
migration work.

The seed also supplies an independent grammar comparison in
`scripts/lang-migration-test.py`. Production syntax and public Lang methods live
in [modules/lang.birnam](../../modules/lang.birnam).
