# File IO installation v2

Design [81AS](../docs/design/81as-production-file-io.md) moves System readFile:
and writeFile:contents: intact from Prelude into `birnam-file-io`. Text View v1,
String v1 and Error Value v2 load first; File IO loads before Prelude. Its
initialization enables the provider without allocating Id objects, registering
roots or methods, or opening files.

Attach the immutable process-pinned descriptor through
`birnam_file_io_dependency_v2`. The arm64 layout is 24 bytes: version 2 at 0,
byte size at 8 and install at 16. Null, wrong version/extent, missing install and
retargeting are rejected; the same pointer may attach again. The
independent C mirror (`abi/fixtures/file-io-v2/file_io_v2.h` at `eb219997`) checks this layout.

Install takes const registration/execution IdApi pointers, an execution presence pointer, the selected System
behavior and already interned readFile:/writeFile:contents: selectors. Prelude
retains System identity/adoption and all twelve interns. Registration stays after
Output and before Environment. Retain Prelude's original execution pointer
before either registration callback, even when the first installation reenters
with another registration table. Reinstallation keeps that execution pointer.
Use behavior/selectors synchronously without retaining or reinterning them.

Wrong arity, null carriers and unavailable exact String arguments return current
nil. There is no inherited String recognition or printing fallback. Reads ignore
the receiver, validate path UTF-8, open the file, and read at most one GiB plus
one byte to reject excess length. Failure returns the existing lazy ioError with
"could not read file". Successful bytes remain owned until the live String
constructor returns, including null or alias results and nested callbacks.

Writes borrow both path and contents in that order before validating path UTF-8.
The contents callback may change the earlier path buffer, and that changed path
remains authoritative. The existing fs::write overwrites/truncates exact bytes;
failure uses ioError with "could not write file". Success returns the original
receiver, including null. File and buffer lifetimes and Error factory dispatch
retain their existing callback order; no path normalization or cache is added.

The methods, descriptor and first execution table remain pinned. System identity,
Public selectors and image encoding are unchanged. The
[export inventory](rust-file-io-dylib-v1.json) contains six symbols. Run
`mise run file-io` and `scripts/file-io-image-smoke.sh PREFIX` for focused debug
checks and startup/source-free compiled/saved-image/baked execution.

Design [81BB](../docs/design/81bb-production-namespace-presence.md) versions this
installer to v2. Its install function receives the execution API's explicit
`const NamespacePresenceV1 *` immediately after the execution API pointer.
Capture the pair together at the original first-install point, before callbacks;
later registration does not replace either execution selection. The caller pins
the live descriptor, its context and code for the retained method lifetime.
Each presence check selects the current function/context at invocation.
