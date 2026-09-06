# 72 — Rust mechanical consolidation and bootstrap audit

## Scope

Share implementations without changing language semantics, byte formats,
allocation strategy, runtime authority, or bootstrap ordering. No new dynamic
library or IdApi field is introduced. This is not a performance optimization.

## Implemented

- `birnam-artifact/src/digest.rs` owns the existing handwritten SHA-256
  implementation. Host imports it from Artifact; Codegen imports it through the
  existing provider-free artifact-codec facade. The padding Vec, compression
  algorithm and lowercase 64-byte output are retained. The old host spelled
  the four-byte input load with slice conversion; Codegen used four indexed
  bytes. The shared implementation retains Codegen's spelling. Independent
  sha2 checks cover padding boundaries and larger inputs.
- `birnam-artifact/src/wire.rs` owns scalar little-endian mechanics. The eight
  format modules had 32 definitions, but not four identical operations: five
  formats use checked offset arithmetic and three use direct addition.
  Six shared helpers preserve that distinction; each caller aliases its former
  variant. Format validation and error ordering stay local. Reads still borrow,
  writes still append to the supplied Vec, and independent readers remain
  independent. Golden-format and truncation tests exercise the production
  decoders; scalar tests additionally pin checked overflow and exact bytes.

The earlier review's claim that all scalar readers were identical was incorrect.
Consolidation must not silently change arithmetic failure behavior.

## Stage 0 / stage 1 audit

The current production entrypoint is `scripts/rust-install.sh`. It builds the
Rust driver in a fresh Cargo target. `birnam-build` then builds the Cargo
workspace in another fresh target, stages the host and native libraries, and
uses that staged host to compile `STAGE1_MODULES` from authored Birnam sources.
The stage-1 list includes stdlib, compiled-unit, project policy, control and
application/tooling modules; it does not contain a second authored compiler.
Ordinary and portable C emission are Rust modules in the same `birnam-codegen`
crate, built before that stage-1 loop.

Source sharing is distinct from binary placement: host and native libraries
may each contain statically linked copies of a pure helper. They must not call
through a later-loaded runtime provider just to share source. The existing
artifact-codec facade includes Artifact source without enabling its provider;
the new digest and wire modules preserve that arrangement and relative imports.
Nothing requires an installed Birnam compiler or a stage-1 module to supply them.

## Additional consolidations (implemented 2026-09-06)

### C-string escaping

`Emitter::push_c_string_body` and `portable_c::push_c_string_body` live in the
same Rust crate. They are alternate output paths, not stage-0/stage-1 compiler
copies. No bootstrap reason was found to duplicate their byte-escaping loop.
`c_string::push_c_string_body` now owns the append-to-buffer loop. Both paths
use it without temporary buffers or a new allocation strategy. A separate
formatted-octal test oracle checks all 256 bytes and all 65,536 adjacent pairs
through ordinary emission and portable byte-constructor emission. Escaping code
with different byte policies and the global-name temporary Emitter are unchanged.

### Private ABI carriers

StoreCompensationApiV1 was independently declared by Store and Codegen; the
generation lease was independently declared by the floor and Codegen. Separate
native producers and consumers require the same binary representation, not
independent Rust declarations. They can share definitions through existing ABI
support without adding a runtime dependency. Preserve C layout, calling
conventions, versions, symbols, opaque lease semantics and independent layout
fixtures. `birnam_abi::private` now owns `StoreCompensationApiV1` and
`StacklessGenerationLeaseV1`; the floor retains its existing name through an
import alias. Frozen numeric layout tests pin every field offset, size and
alignment independently of the shared declarations. No exported function,
version check, callback signature, or lease operation changes.
The versioned resumable registration structures are different: their
flat prefixes intentionally preserve generated-C layout and must not be merged
merely because they look repetitive. Design 60J keeps compensation outside IdApi.

### Receipt validators

Bootstrap and publication receipt modules share mechanical path/digest checks.
Repository search found their parse-and-validate entrypoints called only by
their own tests; the actual installer builds its schema-3 rust-install receipt
directly. These validators are not the self-describing project-publication
authority and are not two compiler stages. No metacircularity reason for their
mechanical duplication was found. Keep their public schemas, exact diagnostics,
validation/syscall order and separate policy intact if consolidated; lower
priority than production paths. Lack of in-tree callers does not authorize
removal of the public modules.

The private host `receipt_validation` module now owns those checks; small local
adapters retain each public error type and supply its original diagnostic
variant. Diagnostic format literals are retained verbatim, because replacing
literal text with interpolated text can alter String's allocation capacity even
when the final message is identical. Tests pin path-error capacity as well as
text. The parse-and-validate bodies and public schema declarations are
unchanged. Hashing still uses the original full-file read, sha2 digest and hex
encoding, not the handwritten runtime digest. Tests pin accepted names, digest
case/length, path normalization, exact errors, path rejection before filesystem
access, first missing component, and symlink rejection at leaf/intermediate
components through both public validators. Existing policy tests remain local.

## Initial digest/scalar verification

Focused Artifact, Codegen-runtime and Host tests passed. `mise run verify`
passed the workspace, native-prefix, image/CodeBinding lifecycle, two independent
fixture-free cold builds and relocated-runtime gates. Both cold builds compiled
all 27 stage-1 modules from source. `mise run test` passed all 611 language tests.
Supplemental logs are in `/tmp/birnam-rust-dedup.kfUBa3/`.

Source inspection establishes no algorithm/allocation changes; it alone does
not prove identical whole-program machine code or wall-clock timing. An isolated
release assembly comparison of the former host digest against the shared body
found different instruction selection despite equivalent source operations;
therefore this change does not claim instruction-identical host hashing.

The focused interleaved before/after digest timing probe was not admitted: the
unchanged host-idle/process-load preflight did not become quiet within the
bounded 60-second wait. No timing sample is accepted and no measured performance
neutrality claim was made from that attempt. The deferred quiet-host repeat
was subsequently completed below; no implementation remains pending.

## Final consolidation verification

Focused Codegen, ABI, Host, Store, floor and Codegen-runtime tests pass, including
the new escaping/layout/receipt regressions. The final `mise run verify` passes:
workspace tests, native-prefix and image/CodeBinding lifecycle gates, two fresh
fixture-free cold source builds with all 27 stage-1 modules each, identical
source/output closure inventories, and relocated-runtime checks. These results
cover the final literal-preserving receipt implementation, not just the earlier
digest/scalar slice. Log: `/tmp/birnam-design72-final-verify.log`.

The final `mise run test` passed all 611 language tests, with zero failures.
Log: `/tmp/birnam-design72-final-language.log`. Formatting and `git diff --check`
also pass. All build/runtime checks used private temporary prefixes; the user's
installed runtime was not replaced.

The 2026-09-06 digest repeat passed the unchanged AC-power/host-load preflight
(70.86% median idle, hottest process 11.1%). The short probe completed before
one interval sample, which the canonical monitor explicitly accepts on the
immediate preflight. It compared the former host body to the current shared
body, alternating their order over 20 measured rounds after one warmup round.
The baseline body was checked byte-for-byte against the host at `e1bb35f9^`.
Iterations per round were `max(2_000_000 / (length + 64), 20)`; values below are
upper-median nanoseconds per call, with black-boxed input and output. Compiled
with Rust 1.95.0, `--edition=2024 -C opt-level=3 -C panic=abort` on arm64 macOS.

| Input bytes | Former host ns | Shared ns | Change |
| --- | --- | --- | --- |
| 0 | 288 | 290 | +0.69% |
| 64 | 517 | 521 | +0.77% |
| 1,024 | 4,063 | 4,085 | +0.54% |
| 65,536 | 245,050 | 246,095 | +0.43% |

This closes the deferred focused timing repeat, not whole-runtime performance
qualification or a claim of literally identical timing. Separately, an isolated
release assembly comparison of the former portable escaping loop and the new
shared call produced identical 203-line function bodies after normalizing local
basic-block, constant-pool and linker-hint labels. This supports unchanged
escaping mechanics but is not a whole-program machine-code comparison.
Supplemental timing/assembly logs and probes are `/tmp/birnam-design72-*`.
