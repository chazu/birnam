# 57 — Project Policy in Birnam

> **Status:** Complete at its deliberately limited source/build boundary,
> revised 2026-09-03. Resolution, dependency-aware artifact invalidation,
> and publication authorization execute in
> `src/id/project-policy.birnam`. The former proposal to extend this policy into
> install/run artifact selection was withdrawn after Design 54 made installed
> artifacts self-describing.
>
> **Publication amendment (2026-09-03):** Design 65 removed persisted project
> build records. ProjectPolicy now receives an ephemeral host observation of
> the previous publication's descriptors solely for incremental build choices.

This slice advances the self-hosting goal without moving effectful operations
into the image. The host remains responsible for filesystem traversal and
source/CST observation, hashing, compiler and wrapped-process launch, file
copying, and the final atomic rename. Birnam owns the
decisions made from those observations.

## Boundary

The project adapter produces a value-shaped snapshot containing:

- discovered projects, manifests, source declarations, and dependency origins;
- resolved module metadata, ordered source parts, source/revision digests;
- an ephemeral view of prior published artifacts, derived from their embedded
  descriptors and current bytes.

`ProjectPolicy` consumes that snapshot and returns values. It owns provider
selection, dependency closure and diagnostics, dependency-first ordering, reuse
versus compile decisions, dirty-dependency propagation, and the publish
decision.

The seam is intentionally narrow. A policy call is one in-process message over
an already collected value; it does not add filesystem traversal or process
launch overhead to the build hot path. The host still performs the expensive
effects only for the actions Birnam selected.

## Artifact rules

An artifact is reusable only when the policy finds a compatible prior
publication, matching ordered source and dependency metadata, unchanged
observed bytes, and no dirty dependency.
A compile decision marks the module dirty in dependency-first order, so later
dependents compile while unrelated modules remain reusable.

The policy returns an ephemeral build plan. Publication is a second policy call
after staging; only then does the host perform the rename transaction. No plan
or inventory is serialized. Later consumers discover the closure from each
artifact's embedded ID descriptor.

## Tests and terminal boundary

`modules/project-policy-test.birnam` covers resolution diagnostics, clean reuse,
dependency invalidation, shared-dependency deduplication, and publication
rejection/approval.
The project smoke test continues to cover source-part changes, tamper
invalidation, wrapped libraries, run modes, and failure preservation.

There is no runtime-policy continuation to this design. Source-oriented
build may use `ProjectPolicy` to choose a local publication plan. Once a path
has been selected, the artifact's embedded descriptor
is authoritative: the ID-floor inspector validates identity, target, ABI,
dependency closure, native requirements, and integrity before `Module`
activation. Run and installed execution do not load `ProjectPolicy` or project
TOML.

Likewise, native-library validation belongs to artifact preflight, not project
policy. Keep hashing, compiler invocation, filesystem copying, dynamic loading,
and atomic rename in host adapters. Moving any of those effects into Birnam for
symmetry would enlarge this seam without adding a user-facing capability.

The implemented source/build slice remains because it is executable dogfooding
of Birnam's project model. It is not an active product milestone. Any future
expansion requires a concrete source-workflow rule that cannot be expressed by
the existing resolver or build planner; runtime artifact selection is explicitly
not such a rule.
