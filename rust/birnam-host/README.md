# Source host over Id

The migrated host loads Id through Siphon, then asks Id to load Bootstrap from
the executable-relative `lib` directory. It uses owner messages for namespace
lookup, roots, GC settings, String construction and Codegen evaluation. Host
values remain rooted across sends; native byte borrows are copied before another
send. The runtime stays on its creating thread in the existing process-owned
domain. It does not unload native images or create isolated runtimes.

`birnam file.birnam` evaluates a regular source file (maximum 16 MiB), with its
path supplied to Codegen for diagnostics and relative embedding. Nil and false
are successful results. Compiler diagnostics, returned language Errors and native
failures exit unsuccessfully on stderr; ordinary program output goes to stdout.
Source arguments are not yet supported. Help and version work without loading Id.

Default startup records and evaluates the authored Module manager and stdlib,
marks the native kernel modules, and sets Module's library directory. This source
bootstrap can be slow in debug builds. `BIRNAM_NO_STDLIB=1` skips both authored
modules; `BIRNAM_GC_PRECISE=1` enables precise payload tracing. Host lifetime tests
also force `collectPrecisely`, excluding conservative native-stack roots.
`BIRNAM_FORCE_SOURCE` is redundant: this slice always boots from source.

The installer builds only the migrated native modules and CLI. It preserves
manifest verification, target locking and atomic prefix publication, and records
source bootstrap in its receipt. It does not compile legacy stage-one dylibs.

Image entry, REPL, project commands, AOT compilation and workers fail explicitly
before startup. The uncompiled `interaction.rs`, `project_{data,files,runtime}.rs`
and `host_bridge_contract.rs` are reference material for those later slices.
Their prior orchestration is in commit `cf5a3b1a`; their tests are not current
acceptance evidence. Whole-workspace migration remains unfinished.

Run `mise run host-cli` for a real install, receipt verification, relocation,
source/stdout/stderr/exit checks, stdlib execution and collecting-host tests.
Run `cargo test --offline --locked -p birnam-host -p birnam-cli -p birnam-build`
for resolver, parser and receipt/publication unit checks.
