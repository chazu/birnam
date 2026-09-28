# System file I/O over Id

Bootstrap loads `file_io` after System, input and environment. Initialization takes
Namespace, installs `readFile:`, `writeFile:contents:` and `embedFile:` through Id sends and
returns Unit. Reload preserves replacements. The module depends only on the common
Id ABI; native filesystem operations stay private to this owner.

The read/write methods require exact String arguments and use current `bytes` messages.
Wrong arity or unavailable/non-String arguments return nil. Paths require UTF-8;
contents remain binary. `readFile:` retains the one-GiB limit and constructs the
result through current `String fromBytes:`. `writeFile:contents:` creates/truncates
the destination and returns its receiver. Failures keep `ioError` with
`could not read file` or `could not write file`. Error lookup remains lazy, after
constructing its kind/message; missing factories return nil. Native send failures
propagate unchanged.

The filename is copied immediately after its byte read, before reading contents.
A contents callback can no longer redirect the write by mutating the earlier
borrowed filename. This explicitly replaces the old native borrowed-path behavior
with the current String lifetime contract. Argument reads remain path-first, then
contents; UTF-8 path validation follows both reads, preserving error precedence.
Contents bytes are also copied before any subsequent send. Receivers and arguments
remain rooted through collecting methods; no foreign payload access, dependency
table or native callback crosses a module boundary.

`bash scripts/native-message-test.sh` includes the module's export/install-name/
dependency audit and the `system_services` test. Real temporary-file checks cover
source calls, all byte values, empty files/truncation, aliases, exact String
recognition, malformed paths, missing files/directories, collecting methods,
exact native failures, lazy/replaced Error factories and reload preservation.
A replacement contents-byte method mutates the earlier path buffer and collects;
the test verifies the original destination is written and the redirected one is
absent. Failed contents sends create no file.

`cargo test --offline --locked -p birnam-file-io --lib` checks the bounded reader's
exact-limit, excess-byte and read-error behavior with small inputs; the production
caller passes the unchanged one-GiB limit. Full Host/CLI and image acceptance remain
separate migration work.

`embedFile:` reads a resolved UTF-8 String path, capped at 16 MiB, and sends
`Bytes fromBytes:` to return an owned Bytes object. Path/content snapshots precede
further sends. Read/limit failures return `ioError`; native dependency failures
propagate unchanged. The compiler owns literal-path validation and source-directory
resolution, then invokes this method by a live send. An ordinary dynamic send to
this method performs file I/O at that moment; it is the compiler's literal form
that captures bytes into code. See [embedded files](../../docs/design/56-embedded-files.md).
