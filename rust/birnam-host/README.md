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

Normal startup restores `lib/birnam-bootstrap.image`; CLI tooling uses
`lib/birnam-tools.image`. Host then rebases Module's library directory. The
installer prepares the Module manager, stdlib,
lifecycle policy, console dependencies and compiled Alien wrappers. The separate
tool image adds the remaining authored tools without expanding the console
generation. Startup maps their native code without invoking the compiler. Missing or incompatible bootstrap
images fail with a reinstall diagnostic; there is no source fallback.
`BIRNAM_FORCE_SOURCE=1` explicitly selects the independent source bootstrap.
`BIRNAM_NO_STDLIB=1` skips the authored bootstrap;
`BIRNAM_GC_PRECISE=1` enables precise payload tracing. Host lifetime tests
also force `collectPrecisely`, excluding conservative native-stack roots.

The installer builds the native modules, CLI and authored tooling artifacts,
then saves the bootstrap image before atomic publication. Its receipt covers the
compiled bootstrap and source recovery modules as well as the tooling artifacts.

`birnam file.image` boots native modules, restores the image through System, then
sends `value` to global `main`. It does not evaluate the source stdlib. Restored
Module managers receive the current executable's library directory. Non-callable
main, returned Errors and image admission failures exit unsuccessfully;
nil and false results succeed. Images without native code retain Image's existing
rebuild behavior; compatible saved code loads without compilation. Image arguments
are not yet supported. Images without main enter the console when they contain
the authored bootstrap. Native-only images require explicit source recovery with
`BIRNAM_FORCE_SOURCE=1`; Host preserves their state instead of replacing it with
the installed bootstrap snapshot. `--rebuild` requests recompilation and `--bake`
appends an image to an executable, recovered through the same System messages.
Image maps saved Alien wrapper objects in its publication transaction and binds
foreign addresses from the current process. Images containing older textual Alien recipes
need to be regenerated with the current Birnam. Image completes composed-generation adoption before
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

Console, socket, image-main and worker entry switch Codegen to running mode at
the idle Host boundary. Ordinary tracked method edits then return
`generationNotQuiescent`; candidate publication and Image recovery use explicit
internal extents. Source bootstrap remains open while initial modules load.

Image `main` also returns to Host before intent fulfillment and checkpoints
Isolate after either successful entry or failure.

The socket REPL seals the initial generation and uses Host-owned accept/session
loops. Each authored read/evaluation step returns before intent fulfillment and
printing; turn roots are released before Isolate checkpoints. EOF, quit and client
faults close that connection before the next accept.

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
