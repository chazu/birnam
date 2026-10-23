# System input over Oop messages

System input now uses the v2 Oop-only module interface. Method contexts are the
owning Namespace object. One ordinary root Sequence retains the completed native
module dependencies and private error-construction Method, including after public
method replacement or partial publication. Id keeps incoming call objects alive.
The error helper receives its message as receiver, protecting it through lazy Error
lookup without retain/release cleanup calls. Environment migration is separate.

- `System readLine` reads binary stdin through the existing buffered reader, removes
  only a final LF, and constructs a String through current `String fromBytes:`.
  CR, NUL and non-UTF-8 bytes remain intact. EOF returns nil. Read failure produces
  `ioError` with `stdin read failed`. Wrong arity returns nil without consuming input.

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

Run `bash scripts/oop-id-slice-test.sh`. Its subprocess checks cover binary input,
empty lines, CRLF preservation, unterminated input, EOF, directory-stdin errors,
absent Error, a captured Method after removal, GC during construction, current
String factories, and reload preserving removal. Full Bootstrap/source/image/worker
qualification remains pending.
