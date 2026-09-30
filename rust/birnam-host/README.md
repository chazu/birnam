# Source, image, artifact and project host over Id

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

Source-file startup records and evaluates the authored Module manager, stdlib and
CompiledUnit lifecycle policy,
marks the native kernel modules, and sets Module's library directory. This source
bootstrap can be slow in debug builds. `BIRNAM_NO_STDLIB=1` skips these authored
modules; `BIRNAM_GC_PRECISE=1` enables precise payload tracing. Host lifetime tests
also force `collectPrecisely`, excluding conservative native-stack roots.
`BIRNAM_FORCE_SOURCE` is redundant for source files and does not request image rebuilding.

The installer builds the migrated native modules, CLI and authored tooling artifacts. It preserves
manifest verification, target locking and atomic prefix publication, and records
source bootstrap and compiled tool identities in its receipt.

`birnam file.image` boots native modules, restores the image through System, then
sends `value` to global `main`. It does not evaluate the source stdlib. Restored
Module managers receive the current executable's library directory. Non-callable
main, returned Errors and image admission failures exit unsuccessfully;
nil and false results succeed. Images without native code retain Image's existing
rebuild behavior; compatible saved code loads without compilation. Image arguments
are not yet supported. Images without main enter the console; native-only images
bootstrap the source stdlib first. `--rebuild` requests recompilation and `--bake`
appends an image to an executable, recovered through the same System messages.
Host replays saved Alien wrapper recipes before user entry so foreign addresses
are resolved in the new process. Image completes composed-generation adoption before
Host enters main, the console or a worker; saved rollback pointers are not reused.

The private `--worker IMAGE RECEIVER SELECTOR --channel-fd FD` entry restores an
image and enters Isolate without running global main. Boot errors use the inherited
channel; Host teardown shuts down child workers. See [Isolate](../birnam-isolate/README.md)
for request, cancellation and lifecycle acceptance.

The project-source adapter restores `index_project_source`,
`observe_project_source` and `project_embedded_files`. It reads one bounded file
snapshot and copies Codegen's metadata and asset inventory through Id messages.
No source is evaluated or registered, no asset is read, and returned observations
contain only owned Rust data. Unindexable source answers None; this is static
discovery, not a syntax or compile check.

Project `check`, `build`, `run`, `fetch`, `install` and `test` use scoped Id
messages with authored TOML, Module composition and ProjectPolicy. Check compiles
selected modules without activation or publication. Build stages and preflights
the complete publication before replacing the previous build. Ordinary run uses
published configuration and bundled application code without reading source;
`--build` explicitly rebuilds. Only fetch acquires missing pinned Git checkouts.
Install copies the planned module/native closure. Test launches a fresh worker
per test source against the published module root, without running application main.

The console keeps a session root plus a fresh scope per turn and checkpoints
Isolate after evaluation. A returned EvolutionIntent is fulfilled through Codegen
at the idle owner-thread boundary before presentation; source-file results use
the same path. Fulfillment runs the authored policy inside a scoped quiescent
extent that closes on success, language Error or native failure. The console
initializes its generation after session configuration. Project tty/socket entry uses the published context
without applying caller Config or running main again. Tool loading temporarily
selects the installed library directory, then restores the project's directory.
LSP receives scoped Project callbacks, revoked on return or unwinding; retained
callback receivers fail closed after the Host borrow ends. Doc, docgen, init and
wrap invoke authored Cli methods directly. Whole-workspace migration and broader
generation replacement remain unfinished.

Run `mise run host-cli` for a real install, receipt verification, relocation,
source/stdout/stderr/exit checks, stdlib image recovery, project workflows and
collecting-host tests. Project acceptance covers path and pinned Git dependencies,
reuse, failed-build preservation, wrapped native libraries, install and relocation.
The image fixture disables compilation and removes global main before collecting
at its entry; the Host must retain the entered block itself.
Run `cargo test --offline --locked -p birnam-host -p birnam-cli -p birnam-build`
for resolver, parser and receipt/publication unit checks.

`birnam --compile-module SOURCE OUTPUT` compiles without source bootstrap or
source evaluation. `birnam file.dylib` boots the authored Module manager/stdlib,
sets its library directory beside the selected artifact, validates the complete
dependency closure, then activates it dependency-first. The root filename may be
arbitrary; dependencies use `libbirnam_NAME.dylib`. Missing requirements, cycles,
identity mismatches and legacy capabilities fail before any planned initializer.
Activation failures retain ordinary already-published effects.

These generated `.dylib` files contain relocatable TCC objects with the
`id-object-v1` contract, not OS shared libraries. Original source and embedded
files are unnecessary at activation. Initial-generation selection remains an
explicitly deferred option; generation replacement remains outside this slice. `scripts/artifact-cli-test.py` verifies fresh-process artifact
execution as part of the installed, relocated Host suite.
