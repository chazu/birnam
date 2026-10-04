# Frozen ImageFormat reference

Standalone test oracle outside the production workspace. It freezes the former
Rust container encoder and admission rules. The production image codec retains
only record-value helpers used by live object projection/restoration.

`cargo test --manifest-path tests/reference/image-format/Cargo.toml --offline --locked`
runs the original unit tests. `mise exec -- python3 scripts/image-format-test.py PREFIX`
compares installed Birnam admission and exact composed bytes on the primitive floor.
