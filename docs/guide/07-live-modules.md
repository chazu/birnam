# File-backed live modules

L1b turns the current dispatch generation into an ordinary editor-friendly
workflow. It exports exact authored source, imports candidate-safe edits without
running them, exposes the delta for inspection, activates only at a host-owned
top-level boundary, and saves full image baselines suitable for isolate
replacement.

## Export and edit

Export one module from the current generation:

```text
[LiveModules fileOut: #pricing to: "/tmp/pricing.birnam"]
```

The first line is metadata owned by the evolution workflow:

```text
// birnam-live-base sha256:<64 lowercase hexadecimal digits>
```

Every byte after that newline is the exact authored module source. Comments,
spacing, declaration order, and the final newline are preserved. Edit that file
with an ordinary editor, but leave its first line intact.

L1b accepts one canonical `Module named:requires:` declaration and declarative
`Reflect defineMethod:` or `Reflect removeMethod:` operations over stable named
targets. Changed top-level effects, new globals, module introduction,
initialization, and migration are rejected rather than partially applied.

## Stage and inspect

Import is inert:

```text
let candidate := [LiveModules import: "/tmp/pricing.birnam"];
[candidate unitName];
[candidate authoredSource];
[candidate expectedBaseDigest];
[candidate additions];
[candidate replacements];
[candidate removals];
[candidate dependencyChanges];
[candidate missingDependencies];
[candidate isMapped]
```

It parses the envelope and whole document, validates canonical identity, compiles
to a temporary candidate bundle, verifies the artifact and manifest, and retains
the candidate for inspection. It does not map native code, run dependency
initializers, change dispatch, or update current source authority.

An unchanged file answers a `LiveImport` whose `status` is `#identical`.
Ancestor-based edits answer `#staleAncestor`; an otherwise valid base outside the
current predecessor lineage answers `#siblingDivergence`. Missing and malformed
envelopes, wrong expected names, unsafe top-level forms, dependency failures, and
artifact mismatches remain distinct kinded errors.

## Activate and roll back

The console provides the host-boundary workflow:

```text
:live stage /tmp/pricing.birnam
:live activate /tmp/pricing.birnam
:live rollback
```

`stage` only imports and inspects. `activate` prepares dependencies, maps and
validates the candidate, returns an `EvolutionIntent`, and lets the Zig host
fulfill it synchronously after the console turn has returned and native
quiescence is active. The final result prints before another form is read.

The programmatic equivalents are `[LiveModules activationFor: candidate]` and
`[LiveModules rollback]`. They return intents; ordinary running code cannot
commit around the host guard. Rollback remains process-local and available only
while the exact predecessor capability and binding ledger remain valid.

Dependency preparation is explicit. `[candidate prepareDependencies]` uses
ordinary `Module load:` behavior and returns its original kinded error. Mapping
with missing dependencies answers `#dependenciesMissing` and leaves the candidate
inert.

## Save a worker baseline

Save only a full image after reconciliation:

```text
:live save /tmp/pricing-worker.image
```

or produce the intent directly:

```text
[LiveModules saveBaseline: "/tmp/pricing-worker.image"]
```

The host detaches process-local console handles for the synchronous save and
restores them immediately afterward. The result reports `#status`, `#path`,
`#baseDigest`, and `#sourceCount`. A successful baseline boots through both
mapped and `--rebuild` recovery with the same complete source map, generation
digest, and behavior, and can be passed directly to `Isolate start:selector:image:`.

Append persistence is not a live-generation durability action. L1b adds no file
watcher, forced replacement, source-aware `adoptLive`, initializer, migration,
promise, background activation, pool policy, or cross-isolate atomicity.

See `examples/live-modules/` for a runnable export, edit, activation, baseline,
mapped/rebuilt boot, isolate request, worker-local L2 rollout, receipt inspection,
and digest-bound rollback.

## Prepare a copied isolate rollout

An inspected L1b candidate can also become an L2 copied-value request without
mapping or activating it in the coordinator:

```text
let candidate := [LiveModules import: "/tmp/pricing.birnam"];
let request := [LiveModules evolutionRequestFor: candidate];
let receipt := [worker evolve: request timeout: 10000]
```

`evolutionRequestFor:` publishes the verified dylib and pointer-free sidecar under
the artifact checksum, then returns the fixed positional activate record. The
worker independently verifies its expected base, module and revision identity,
source, checksum, content-addressed path, dependency state, bindings, and
candidate manifest before committing. The coordinator's candidate and the
worker's prepared capability are separate process-local objects.

Use `[LiveModules rollbackRequestForDigest: digest]` to construct a rollback
control from an accepted receipt. Rollback availability is local to the worker
process and is intentionally lost across save/boot, even though the accepted
generation itself remains durable.

From the repository root, `bash examples/live-modules/run.sh` executes both the
host-side L1b lifecycle and the worker-side L2 lifecycle. The rollout portion is
kept in `rollout.input` so the coordinator itself boots from the exact v1 baseline
whose digest the worker compares.
