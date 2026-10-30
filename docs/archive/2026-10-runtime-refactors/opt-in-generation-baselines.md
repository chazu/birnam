# Opt-in generation baselines and observational validation

> Archived 2026-10-30. Historical decisions, implementations and measurements;
> not current architectural authority or a work queue. Statements such as
> “current,” “required,” and “remaining” describe the recorded period.
> Use [Birnam invariants](../../../INVARIANTS.md) for current requirements.

2026-10-17. Follows [explicit image adoption](explicit-image-adoption.md).
Implemented and qualified; uncommitted. No further Store migration in this slice.

## Changes

Console startup, TCP REPL startup and Concurrency worker-image preparation no
longer adopt a generation. `DispatchGeneration adoptCurrent` is the sole authored
entry to the existing baseline workflow; redundant `initializeCurrent` is removed.
The console no longer requires a DispatchGeneration global merely to start.
Worker source preparation is owned by each Concurrency program, not by a global
one-generation gate. Repeated preparation remains subject to normal imageability
and live-resource rules, without an artificial `concurrencyAlreadyPrepared` check.

Source registration and ordinary AOT loading are never sealed. The seal flag,
queries, setters, native loader admission checks and corresponding formal transition
are removed. Duplicate-source/metadata/dependency/ABI checks remain. Removing a
seal does not authorize a malformed artifact or replay a failed initializer.

The image source-registry section is now
`[#sourceRegistryV1, [moduleName, compilationRecord], ...]`. Both readers validate
the tag. It replaces the old Boolean policy header; old images must be regenerated
with this runtime, rather than silently retaining or ignoring an obsolete policy.
The outer image container and Id ABI do not change. Source compilation metadata,
identity, owner revisions and atomic image exchange still apply.

`Generation reconcileCandidate:` and strict validation compare current state with
the explicit baseline. They do not call lifecycle setters, cache a drift bit, mark
all units drifted or revoke rollback. Restoring an identical binding can restore
agreement after a reported mismatch. Source registration changes still invalidate
the source-lineage observation, including A-to-B-to-A replacements. Ordinary sends
remain late-bound; conditional undo checks its own selected state.

## Deliberate remaining boundary

Explicit candidate operations still require an adopted source baseline and carry
the existing whole-source predecessor/selection contract. New module registration
is allowed after adoption, but changes that lineage. It does not auto-adopt the
new module, update old artifacts or make an old prepared token valid. In-place
baseline renewal remains absent; this needs to be addressed when narrowing the
candidate contract, not papered over with a new global reset/transaction service.
No claim is made that the full managed ledger or its retention is now gone.

Next: finish that candidate/source-lineage simplification, qualify activation in
entered code before removing Host-only intent fulfillment, and separate logical
discard from file cleanup and diagnostics. Image capability handoff, native code
ownership, roots, lookup invalidation and deoptimization remain intact.

## Acceptance

- Console and TCP start with no generation; explicit adoption then permits normal
  activation and exact rollback in the real Host.
- Ordinary source registration/artifact loading remain available, including after
  prior module registration. Duplicate sources and corrupt artifacts still reject.
- Explicit validation cannot invoke the status setter or disable a live inverse;
  restoring the binding permits validation again. Source revision conflicts remain.
- Multiple Concurrency program preparations work without adopting; source-free
  workers, service requests, structured close and temporary-workspace cleanup work.
- Mapped/rebuilt/resaved images use the new registry tag, preserve metadata and
  reject the obsolete shape. Startup still does not parse or compile source.
- Lean/TLA+ cover no implicit adoption, observational certification, unrestricted
  registration, restored-value agreement and relevant source revisions, with fault
  variants that restore the removed policies. These models are abstract contracts.

## Qualification

The isolated build `/tmp/birnam-opt-in-baseline/prefix` passed production bootstrap
and image preparation (38 native libraries, four source bootstrap modules and 23
authored tools). Parser/compiler-disabled startup passes, including rejection of
the obsolete registry header by both projection and image staging.

- All 826 language assertions and 61 compiled-unit CLI policy checks pass.
- Real console and TCP tests start without adoption, then explicitly adopt and
  activate/roll back. Multiline input, diagnostics and successive TCP sessions pass.
- Two Concurrency preparations coexist with no generation; all 19 worker-service
  checks pass and both workspaces are removed. Twelve worker wire cases match the
  previous image-adoption runtime byte for byte.
- Native transaction, observational-ledger, retirement, source-loader and artifact
  activation tests pass. Explicit validation of outside drift leaves rollback
  usable; the old runtime failed the no-status-setter regression.
- Native images, the 21-epoch image chain, Host mapped/rebuilt/resaved images,
  explicit adoption, live-load failures and divergent descriptions pass. The image
  chain retains a fixed live set and releases fixture mappings.
- Lean checks 358 theorems and rejects 103 mutations; TLC obtains all 228 expected
  outcomes. The removed sealing transition/control was replaced with the new
  certification/startup/registration model and fault variants.

The initial TCP test reached the expected incomplete-input timeout but its Python
handler did not catch this interpreter's `socket.timeout` class. It now catches
that class as well as `TimeoutError`; the socket-only rerun passed. Console checks
had already passed in the original run. No product workaround was required.

See the [receipt](../../measurements/opt-in-generation-baselines-20261017.json) for
commands, logs and hashes. This slice and the preceding image-adoption slice remain
uncommitted; the user's installation is unchanged. No performance claim is made.
