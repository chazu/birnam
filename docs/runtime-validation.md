# Runtime split validation

Recorded 9 November 2026 on macOS 27.2 ARM64 with the pinned toolchain in [toolchain.json](../toolchain.json). The final source/build-input SHA-256 is `ca656c16b0a6f79b4a3c70c6cdd2757a0174aba3c90cec5d4808efe8d4f006c2`, computed as in the [Id receipt](id-validation.md): sorted relative paths, a NUL, file bytes, and another NUL for `Makefile`, `toolchain.json`, and every non-`__pycache__` file under `src`, `include`, `tests`, `examples`, `modules`, and `tools`. Documentation is excluded.

This receipt covers the [runtime services](runtime.md) split: `seed.dylib` divided into `runtime.dylib` and an interpreter-only `seed.dylib`, Lang's default translator resolving `[Seed syntax]` at use, and the native toolchain running without the interpreter loaded.

## Generated-C oracle

The split was required to leave the authored compiler's output unchanged. The Stage 1 C for each toolchain module, emitted by the interpreted compiler, was compared with the hashes in the previous bootstrap receipt.

| Module | Stage 1 C SHA-256 | Against previous receipt |
|---|---|---|
| id | `3e5c6c580a9bca1794fe7c6b5320c7f84e4d5ee9bd45a385f6bb4bb3a84403d8` | identical |
| peg | `ba3d5d22480b107077f4a6a9fd375d706a83db1a72d655834b233273e5db6d08` | identical |
| lang | `ed5774597b242d789674839060f443c36e7517c95bf120f03725ffd887acb475` | identical |
| lang-syntax | `d913f32051f9ecfba302a0e4b5e02ef2fcfda53a21ec500170d77775ec148330` | changed; was `bf97abb5…86da` |
| compiler-ir | `ccb45b0cb29c8339990ef8e07ae52014a2032629c1cb6bcee2ad396049206328` | identical |
| compiler-c | `ef955d97e907d06d822300000720eb4402e1be671c47ffa3e30866254f919319` | identical |

After the Odin split alone, all six hashes were identical to the previous receipt. `lang-syntax` changed afterwards because its source changed: the load-time `let syntax := [Seed syntax]` was replaced by a send inside the default visitor's `syntax` Method. The other five stayed identical through that change and through the native-chain change.

## Regression gate

`make verify` passed on the final tree; one run took 3m18s. It was run before the work started, after each of the three code changes, and once more after the documentation.

New observations in the gate:

- `runtime_contract.c`, loaded after `runtime.dylib` alone: the 8 published services were bound, `Seed` was unbound, Integer arithmetic worked, the prototype Block's value was nil, and a second load of `runtime.dylib` changed nothing.
- `siphon seed.dylib` without the runtime failed with `unknownName`.
- The compiled `42` example ran on `runtime.dylib` with the absence witness loaded first and last. The witness rejected a process with `seed.dylib` loaded, with `interpreterPresent`.
- The compiler and Id native acceptance ran on `runtime.dylib` alone under the reworked compiler guard, which requires `Seed` to be unbound. The compiler contract finalized 200 of 200 closures and reported no interpreter fallback. The Id contract kept 1000 definitions in one layer and finalized 4095 of 4096 specialized objects.
- `tests/lang/takeover.birnam` gained an assertion that the default translator observes a replaced `Seed syntax`. Run against the previous `lang-syntax.birnam`, that assertion fails.

All earlier siphon, seed, PEG, Lang, compiler, and Id acceptance passed with `runtime.dylib` loaded before `seed.dylib` on interpreted paths.

## Native bootstrap

`make verify-selfhost` passed in 23m33s; the bootstrap pipeline took 1216s. Stage 1 ran on `runtime.dylib` and `seed.dylib`. Stage 2, stage3-c, the compiled application, the five acceptance corpora, and the five rejected inputs ran on `runtime.dylib` with no `seed.dylib` on the command line and the absence witness loaded before and after the native work. Stage 1, Stage 2, and stage3-c C matched byte for byte for every module, and the Stage 2 compiler built and ran the `42` example. The missing-module failure, the public CLI, and the witness's rejection of a process with the interpreter loaded all passed. The receipt records `interpreter_absent: true`, `runtime_sha256`, and `seed_sha256`.

`python3 tests/selfhost_test.py --existing` then validated that receipt against the same binaries and reran the acceptance, and passed.

Observed per-module times, including emission and Clang, compared with the [previous receipt](id-validation.md):

| Module | Stage 1 seconds | Previous | Stage 2 seconds | Previous |
|---|---:|---:|---:|---:|
| id | 20.657 | 30.009 | 74.152 | 78.550 |
| peg | 45.897 | 64.619 | 163.436 | 173.710 |
| lang | 16.280 | 23.298 | 63.949 | 64.636 |
| lang-syntax | 23.728 | 33.713 | 92.462 | 96.945 |
| compiler-ir | 11.549 | 17.076 | 45.238 | 46.124 |
| compiler-c | 28.846 | 41.208 | 102.657 | 105.754 |

These are single runs on different days and are not a controlled comparison. Nothing in the split was intended to change speed, and the differences have not been investigated.

## Boundary

This qualifies the module split and the interpreter-free native load chain on this Mac. The runtime services remain Odin, and the interpreter is still required to build Stage 1 from source. The witness observes the `Seed` binding, not which libraries are mapped. `--eval` with the runtime loaded but no seed is not tested. Nothing observes the prototype Block's code answering `source` or `forms`, which it no longer does. The binaries are not reproducible across rebuilds: the final `make verify` rebuilt them, so the local bootstrap receipt's artifact hashes no longer match `build/` and `--existing` needs a fresh `make verify-selfhost`. Generated artifacts and receipts remain local build products.
