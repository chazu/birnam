# Environment installation v2

Design [81AR](../docs/design/81ar-production-environment.md) moves System getEnv:
intact from Prelude into `birnam-environment`. It depends on Text View v1, String
v1 and Error Value v2 and loads immediately before Prelude. Initialization only
enables its provider; it performs no environment lookup and creates no objects,
methods, roots or layouts.

Attach the immutable, process-pinned descriptor through
`birnam_environment_dependency_v2`. Its arm64 layout is 24 bytes: version 2 at 0,
byte size at 8 and one install function at 16. Admission rejects null, wrong
version/extent, missing install and retargeting; repeat attachment of the same
pointer succeeds. The independent C mirror (`abi/fixtures/environment-v2/environment_v2.h` at `eb219997`)
checks layout without Rust headers.

Install receives const registration/execution IdApi pointers, an execution presence pointer, the selected System
behavior and the already interned getEnv: selector. Prelude retains System
creation/adoption, all twelve selector interns and the original binding position,
after writeFile:contents: and before load:. Use the registration table for the
method callback and retain Prelude's first execution pointer before invoking it.
These tables may differ on the first installation because registration can
reenter. Later installation keeps the original execution pointer. Use behavior
and selector synchronously without reinterning or retaining them.

The method ignores its receiver. Wrong arity, null argument carrier or unavailable
exact String name returns current nil. Text View supplies borrowed bytes using
the retained execution table. Invalid UTF-8 names produce the existing lazy
ioError with "could not read environment". A live std::env::var lookup returns
current nil for NotPresent, constructs a String for a Unicode value, and uses the
same ioError policy for NotUnicode. Name syntax and platform lookup behavior are
unchanged; this module adds no validation or normalization.

Successful lookup owns its value bytes until String construction returns. A
constructor can mutate the environment or perform a nested lookup without
changing those outer bytes. Read the current String constructor/context at the
existing point and return its result, including null or an alias. Name-view,
Error and String callbacks retain their existing reentry/GC behavior. No cached
values, environment snapshot, enumeration or setter is introduced.

The method, descriptor and original execution table remain pinned. System
identity, image encoding, public selector and fresh lookup are unchanged.
The [export inventory](rust-environment-dylib-v1.json) declares six symbols. Run
`mise run environment` and `scripts/environment-image-smoke.sh PREFIX` for focused
debug checks and startup/source-free compiled/saved-image/baked execution.

Design [81BB](../docs/design/81bb-production-namespace-presence.md) versions this
installer to v2. Its install function receives the execution API's explicit
`const NamespacePresenceV1 *` immediately after the execution API pointer.
Capture the pair together at the original first-install point, before callbacks;
later registration does not replace either execution selection. The caller pins
the live descriptor, its context and code for the retained method lifetime.
Each presence check selects the current function/context at invocation.
