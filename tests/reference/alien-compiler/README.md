# Alien compiler reference

This standalone, dependency-free Rust package freezes the compiler before its
Birnam rewrite. It is a test oracle: it is outside the production workspace,
exports no native module and is never installed. Keep the grammar, ABI classifier,
emitter and prelude stable so the authored implementation has an independent
behavioral comparison.

Run `cargo test --manifest-path tests/reference/alien-compiler/Cargo.toml --offline --locked`
for the original unit tests, or `mise exec -- python3 scripts/alien-compiler-test.py PREFIX`
for installed Birnam marshalling/diagnostic comparison and ABI checks. The latter
runs without the standard library, outside the checkout, using the compiled
artifact and embedded C assets.

The frozen native adapter uses Operand/Outcome and is not a reference for the
current Oop calling convention. The runner compares the foreign-marshalling bodies
exactly after removing the known ABI-specific preludes and entry/registration
wrappers. It normalizes only the failure guard's `active->failure.status` access
to `active->failure`; diagnostics remain byte-for-byte comparisons. The frozen
grammar, classifier, emitter and prelude are unchanged.

An independent C fixture separately exercises generated Oop modules through real
Alien calls: scalar values, full-width unsigned values, pointers, structs, arrays,
nested homogeneous-float aggregates and collecting callbacks. Do not copy the
production adapter into this frozen oracle to manufacture equivalence.
