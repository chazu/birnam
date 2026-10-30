# Validation

Validate changes against [Birnam's invariants](../INVARIANTS.md) and the relevant
message behavior. A passing implementation test does not authorize a different
language design. Historical test counts and release receipts do not establish
what the current checkout supports.

## Choose checks for the change

Inspect the routine plan without running it:

```sh
python3 scripts/validate.py --suite routine --plan
```

The planner's current source defines its steps and dependencies. `mise run verify`
runs the routine plan; `mise run verify-exhaustive` selects the larger matrix.
Run focused checks while developing, and relevant broader checks when changed
behavior crosses those boundaries. Do not repeatedly run unrelated matrices
without a concrete reason.

Useful focused commands include:

```sh
mise exec -- cargo check --workspace --all-targets --all-features --offline --locked
mise exec -- cargo test --workspace --all-features --lib --offline --locked
bash scripts/oop-id-slice-test.sh
python3 scripts/id-continuation-test.py
bash scripts/oop-bootstrap-test.sh
```

The Oop core script builds explicit native modules and exercises dispatch,
construction, data operations, collection and native-call behavior. Its negative
dispatch case rejects an implicit application protocol. The continuation caller
checks actual Id continuation behavior, including custom lookup and native-call
boundaries. Bootstrap checks cover source construction and compiled startup.
Use the script contents and their produced records for exact coverage, not old
checkpoint summaries.

Some integration tests are ignored by ordinary Cargo runs because they need a
specific runtime setup. The corresponding scripts supply it. A unit-test pass
does not mean those integration tests ran. Relevant examples include
`scripts/native-message-test.sh`, `scripts/host-cli-test.sh`, and
`scripts/birnam-test.sh`; inspect their options before invoking them.

## Behavior and performance

Preserve observable results, effects, errors, identity, replacement, concurrency
and recovery. Exercise collection and reentry where a change affects them.
Tests may assert internal operations when they enforce an actual contract, such
as no dictionary edits during Method construction. Internal allocation histories
or accessor counts are not requirements unless explicitly part of the behavior.
See [optimization behavior](contracts/runtime.md#execution-and-optimization).

Measure performance separately from correctness. Record the code revision,
build, workload, timing scope and relevant machine conditions. Distinguish a
single-operation improvement from startup or application throughput. Keep rejected
experiments and limitations visible in the [measurement records](measurements/README.md).

## Releases and installation

`scripts/release.sh` defines the normal release workflow, including qualification
and archive checks. `scripts/rust-install.sh` builds and installs a prefix and
performs its built-in startup checks. A build or installation alone is not a full
release qualification. Explicit user instructions about what to run take
precedence; record skipped checks honestly instead of fabricating a passed receipt.

## Historical evidence

The former migration-by-migration validation narrative is
[archived](archive/2026-10-runtime-refactors/validation-history.md). Its commands,
coverage claims and unresolved statuses describe particular old revisions.
They are not the current test inventory or an active work queue.
