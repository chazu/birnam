# Source and image host over Id

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

Source-file startup records and evaluates the authored Module manager and stdlib,
marks the native kernel modules, and sets Module's library directory. This source
bootstrap can be slow in debug builds. `BIRNAM_NO_STDLIB=1` skips both authored
modules; `BIRNAM_GC_PRECISE=1` enables precise payload tracing. Host lifetime tests
also force `collectPrecisely`, excluding conservative native-stack roots.
`BIRNAM_FORCE_SOURCE` is redundant for source files and does not request image rebuilding.

The installer builds only the migrated native modules and CLI. It preserves
manifest verification, target locking and atomic prefix publication, and records
source bootstrap in its receipt. It does not compile legacy stage-one dylibs.

`birnam file.image` boots native modules, restores the image through System, then
sends `value` to global `main`. It does not evaluate the source stdlib. Restored
Module managers receive the current executable's library directory. Missing main,
non-callable main, returned Errors and image admission failures exit unsuccessfully;
nil and false results succeed. Images without native code retain Image's existing
rebuild behavior; compatible saved code loads without compilation. Image arguments
and REPL fallback are not yet supported.

The private `--worker IMAGE RECEIVER SELECTOR --channel-fd FD` entry restores an
image and enters Isolate without running global main. Boot errors use the inherited
channel; Host teardown shuts down child workers. See [Isolate](../birnam-isolate/README.md)
for request, cancellation and lifecycle acceptance.

REPL, project commands and AOT compilation fail explicitly
before startup. The uncompiled `interaction.rs`, `project_{data,files,runtime}.rs`
and `host_bridge_contract.rs` are reference material for those later slices.
Their prior orchestration is in commit `cf5a3b1a`; their tests are not current
acceptance evidence. Whole-workspace migration remains unfinished.

Run `mise run host-cli` for a real install, receipt verification, relocation,
source/stdout/stderr/exit checks, stdlib image recovery and collecting-host tests.
The image fixture disables compilation and removes global main before collecting
at its entry; the Host must retain the entered block itself.
Run `cargo test --offline --locked -p birnam-host -p birnam-cli -p birnam-build`
for resolver, parser and receipt/publication unit checks.
