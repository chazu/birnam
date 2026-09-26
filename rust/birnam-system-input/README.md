# System input and environment over Id

Bootstrap loads `system_input` and `environment` after System construction and
printing/output. Each initializer takes Namespace and installs the existing System
method through Id sends. Reload preserves replacements and returns Unit. Both
modules depend only on the common ABI.

- `System readLine` reads binary stdin through the existing buffered reader, removes
  only a final LF, and constructs a String through current `String fromBytes:`.
  CR, NUL and non-UTF-8 bytes remain intact. EOF returns nil. Read failure produces
  `ioError` with `stdin read failed`. Wrong arity returns nil without consuming input.
- `System getEnv:` requires an exact String name, reads it through `bytes`, validates
  UTF-8 and performs the existing environment lookup. Missing values return nil;
  empty values return an empty String. Invalid Unicode in a name/value produces
  `ioError` with `could not read environment`. It adds no environment mutation API.

String construction, exact-String recognition, Namespace lookup, Error construction,
root retention and method installation use Id sends. Temporary receivers, arguments,
error kinds/messages and selected factories remain rooted across collecting methods.
Native send failures propagate unchanged. Error lookup stays lazy: construct the
kind/message first, then look up Error, returning nil if no factory is present.
A factory published while the message is constructed therefore takes effect.

`readLine` retains the existing stdin-lock lifetime through String/Error conversion.
A conversion method that recursively reads stdin can block; this migration does not
change that behavior. Native stdin/environment OS operations remain private to the
module implementing them.

Run `bash scripts/native-message-test.sh`. The `system_services` integration test
uses isolated subprocesses and test-only environment names. It covers binary,
empty, CRLF, long and unterminated input; EOF and directory-stdin failure; missing,
empty, Unicode and non-Unicode environment values; exact String recognition;
collecting and replaced methods; native failures; lazy Error publication; and reload
preservation. Source evaluation also exercises `getEnv:`.

[File I/O](../birnam-file-io/README.md) now uses the same prefix, including an owned
filename snapshot before contents reads. Full Host/CLI startup, System GC controls
and loaders remain separate work.
