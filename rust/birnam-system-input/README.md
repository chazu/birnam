# System input over Oop messages

System input uses the v2 Oop-only module interface and Namespace Method contexts.
A registered owner Sequence retains completed dependencies and reports scoped
temporary roots. Id protects incoming call objects. Error construction is a local
function with an ordinary send to the current Error factory; no private callback
Method is involved.

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
