# 60H — Stackless Production Acceptance and Rollout

> **Status:** Accepted and implemented through H5 on 2026-08-27 after
> decision-by-decision discussion and the incorporated [60HA adversarial
> review](60ha-stackless-production-acceptance-and-rollout-adversarial-review.md).
> H6 remains the release-time designation of one clean immutable archive.
> This is the production gate for the completed S0-S4, 60G, 60J, and 60K
> stackless runtime. It adds no execution semantics, compatibility adapters, or
> language annotations.

## 1. Decision

Birnam makes one exact-current cut to the stackless runtime. The production
runtime admits only current image, artifact, manifest, descriptor, selection,
native-carrier, IdApi, and Runtime-ABI versions. Backward compatibility is not
a product requirement.

The stackless/v7 implementation is simply Birnam. There is no legacy runtime
mode, stackless feature flag, dual writer, compatibility reader, upgrade shim,
or downgrade writer. Synchronous and resumable implementation dispositions
remain ordinary compiler policy inside the one current runtime; they are not
rollout modes.

One immutable release-candidate archive is built, verified, soaked, and then
promoted byte-for-byte. Production readiness is an evidence-backed statement
about that archive, not about a source tree from which different bytes may
later be rebuilt.

## 2. Compatibility boundary

### 2.1 Images

The only admitted durable image format is image-v7.

- New saves write v7.
- v4, v5, v6, unknown, and future versions reject before heap allocation,
  Store mutation, native mapping, or image-recovery publication.
- The runtime contains no v4/v5/v6 admission or recovery reader.
- There is no in-runtime migration or v7-to-legacy downgrade path.
- Historical format descriptions may remain as design provenance. Historical
  bytes may remain only as explicit rejection inputs; they are not positive
  compatibility fixtures.

Version-neutral canonical-field, retained-fact, checksum, and outer-framing
helpers used by v7 belong to the current v7 implementation. Refactoring such a
helper out of a legacy-named module is required before deleting that module;
copying it into two authorities or retaining a callable legacy admission path
is forbidden.

Rollback restores another image-v7 checkpoint admitted by the selected
release. Deleting the only admitted checkpoint is an operator action, not a
condition the runtime disguises with lossy reconstruction.

### 2.2 Compiled and native artifacts

Only the current candidate envelope and manifest, ArtifactSelection,
GenerationSelection, ResumableManifest, FrameDescriptor, portable inventories,
portable native carrier, target ABI, IdApi, and Runtime ABI are admitted.

Obsolete AOT artifacts are caches and are rebuilt from current source or
portable meaning. The runtime does not inspect, activate, or adapt obsolete
candidate-manifest versions. An exact mismatch rejects before mapping or
publication.

IdApi v1 remains named v1 because it is the current native-module ABI. That
name is not a promise to accept a future incompatible module. A future ABI or
format change may make another clean cut, but must change the exact version or
fingerprint, regenerate its fixtures, and rerun this record's complete gate.

### 2.3 Platform

The production claim is arm64 macOS only, matching Birnam's current platform
contract. Another target remains unsupported until its packaged runtime passes
the entire 60H matrix, including native mapping, rebuild, relocation, stack
depth, memory, and longevity evidence. Passing portable codec tests alone does
not promote a platform.

## 3. Failure contract

Every compatibility failure is explicit and kinded. Diagnostics identify the
rejected component, expected exact version or fingerprint, observed value when
safe to report, and the current remedy: rebuild the cache, supply a current v7
image, or use a matching release.

The runtime never:

- reinterprets old bytes as a current carrier;
- silently lowers a requested resumable implementation synchronously;
- substitutes source replay for portable meaning;
- maps native code before complete structural and ABI admission;
- partially publishes heap, Store, dispatch, or CodeBinding state after a
  rejected input; or
- changes meaning merely to make an old artifact boot.

A v7 image with incompatible or absent native code may rebuild the exact
admitted PortablePrograms and materialization selection through a compatible
current compiler. That is v7's normal physical projection rule, not a
compatibility fallback. With neither compatible mapped code nor compiler,
recovery rejects explicitly.

## 4. Acceptance gates

### H1 — Per-change verification

Every proposed runtime change passes:

1. Rust formatting, workspace check, unit tests, and doc tests;
2. the complete Birnam language suite;
3. current portable-format stored goldens, independent reader, generated
   boundaries, and malformed-input matrix;
4. exact rejection tests for v4/v5/v6 images, obsolete candidate manifests,
   wrong target, wrong IdApi, and wrong Runtime ABI; and
5. structural fast-path checks required by 60E and 60F.

### H2 — Release-candidate verification

The candidate archive, not an in-tree development binary, passes:

1. clean production build from the pinned toolchain;
2. archive integrity, ad-hoc signing, clean-environment boot, cold archive,
   relocation, and installed-layout checks;
3. image-v7 mapped, forced-rebuilt, deliberately stripped, resaved, and
   fresh-process recovery;
4. current candidate activation, replacement, exact rollback, discard, and
   physical reclamation;
5. portable Block and CodeBinding publication, replacement, self-removal,
   eviction survival, and quiescent retirement;
6. the complete 60J bounded-churn matrix and retained-history growth control;
7. explicit old/unknown-version and malformed-authority rejection; and
8. the blocking performance policy of §5.

No manual substitution for a failed item is accepted. A platform-specific
gate may be inapplicable only by removing that platform from the release claim.

### H3 — AFK longevity verification

The packaged candidate runs twenty epochs. Each epoch performs at least 5,000
method define/replace/remove operations, 5,000 transient Symbol operations,
and 5,000 bounded-history operations over a fixed live set. Every epoch
compacts, saves v7, exits, and reloads in a fresh process; mapped and forced
rebuild recovery alternate.

After warm-up:

- live heap bytes, Symbol count/name bytes, vtable count, current and retiring
  CodeBinding counts, generated binding bytes, retained native generations,
  semantic Store bytes, image bytes, and process RSS plateau within declared
  allocator slack;
- no metric has an unexplained slope proportional to epoch or process age;
- every reloaded image preserves exact behavior and current binding meaning;
- a deliberately retained-history control grows and attributes that growth to
  its requested retention window; and
- returning to current-only retention compacts the control back to the fixed
  live-set bound.

The arm64 macOS gate uses exact equality for Symbol count/name bytes, vtable
count, CodeBinding counts, generated binding bytes, and semantic Store bytes;
1 MiB slack for live heap bytes; 64 KiB for image bytes; a 192 MiB bound on
process maximum-RSS spread; and a separate 64 MiB first-to-last RSS age-drift
bound. The wider spread allowance bounds allocator and OS page-retention
variance while the stricter drift check prevents it from disguising
age-proportional growth. The complete calibration drive observed flat semantic
and image counters, a 144 MiB RSS high-water spread, and 22 MiB first-to-last
drift across eighteen post-warm-up epochs.

The operation count and repeated process/image lifetimes are the gate. Elapsed
duration is reported but is not treated as proof of indefinite execution.

## 5. Blocking performance policy

Performance is measured on the same supported machine before and after the
candidate, using release builds and the existing 60C/60E/60F probes.

- The ordinary synchronous-send median may regress by at most ten percent from
  the previous production release.
- Existing resumable entry, Call/Return, allocation, and stack-depth budgets
  remain mandatory.
- One million mutual resumable calls retain bounded native-stack movement.
- The 100,000-iteration G0 callback-control proof remains mandatory.
- The resolve-and-pin structural fast path remains free of allocation, locks,
  atomics, and reference counting.
- Longevity duration is report-only; correctness and memory plateau are
  blocking.

An accepted regression requires a new explicit design decision. Updating a
baseline to make a candidate pass is not acceptance.

## 6. Release evidence

The existing install/release receipt is extended into one machine-readable
evidence bundle. It records:

- schema and semantic scope;
- exact Git commit and clean-tree state;
- target and pinned toolchain;
- IdApi fingerprint, Runtime ABI, image version, and every admitted artifact
  version;
- stored-golden and independent-reader checksums;
- packaged executable, dylib, source, and archive digests;
- every acceptance command, exit status, and complete-output digest;
- same-run performance measurements and budgets; and
- every longevity epoch's counters, image size, RSS, recovery mode, and
  retained-history control.

A human-readable summary is generated from that receipt. Neither document is a
second authority. Publication rejects when evidence is missing, stale, refers
to a dirty tree, names different bytes, or omits a required gate.

Release logs may be stored separately, but the receipt hashes them so their
identity is durable without inflating the installed runtime.

## 7. Immutable promotion and rollback

The release pipeline has four states:

```text
source commit
  -> immutable candidate archive
  -> verified candidate plus complete receipt
  -> byte-identical published release
```

Verification never promotes a mutable directory. Promotion copies or renames
the already-digested archive; it never rebuilds it. The release tag names the
same commit recorded in the receipt, and published checksums match the verified
archive and its contents.

Rollback selects another already-published exact-current release and a v7
checkpoint admitted by it. There is no promise that a newly changed future ABI
accepts today's artifacts; its own release must establish its own exact-current
matrix and checkpoint contract.

## 8. Delivery slices

1. **H0 — Contract and review.** Freeze this record and 60HA.
2. **H1 — Exact-current cut.** Remove v4/v5/v6 runtime readers, obsolete
   candidate-manifest readers, compatibility branches, and positive legacy
   tests. Move v7-owned shared helpers to version-neutral ownership.
3. **H2 — Rejection matrix.** Add exact old/unknown-version and stale-artifact
   failure-before-publication gates.
4. **H3 — Evidence driver.** Extend the receipt and add one release-acceptance
   orchestrator over immutable packaged bytes.
5. **H4 — Longevity.** Generalize K6 to twenty fresh-process epochs with
   persisted metrics and the retained-growth control.
6. **H5 — Promotion.** Prove candidate/archive/receipt/tag identity and prevent
   post-verification rebuild.
7. **H6 — Production designation.** Run the complete arm64 macOS matrix and
   publish the exact evidence bundle.

## 9. Implementation checkpoint

The runtime now has one current candidate-manifest reader and one image-v7
reader. V4-v6 image modules, the legacy image-codec crate, replay journals,
source/bootstrap fallback, and obsolete Store/image load protocols were
removed. Early rejection covers old, unknown, and malformed image headers and
obsolete candidate manifests before publication.

Image recovery executes method operations and only the already-admitted
capture-free projected block bindings; it does not rerun arbitrary module
initializers. Store image projection uses explicit method-mirror provenance and
relational `#imageTransient` durability policy. Identity-equal compositions are
re-adopted, while changed compositions retire superseded lifecycle graphs
through ordinary sends. Together these changes make repeated mapped/rebuilt
current-only resaves plateau without sealing the ordinary send surface.

The release install receipt is schema 3 and records exact source, target,
toolchain, ABI, format, source-manifest, staged-manifest, and artifact identity.
Release acceptance operates on an immutable archive, requires an explicit
prior-production Rust baseline, checks baseline/candidate machine agreement,
hashes every gate and log, and emits one authoritative receipt. Promotion uses
those already-verified bytes.

The H4 implementation completed twenty alternating mapped/rebuilt fresh-process
epochs on arm64 macOS. After two warm-up epochs, live heap bytes, Symbol count
and name bytes, vtable count, current/retiring bindings, generated binding
bytes, semantic Store bytes, and image bytes were flat. Maximum RSS ranged from
599,326,720 to 749,895,680 bytes and ended 22,757,376 bytes above the first
post-warm-up epoch, satisfying the separately declared spread and age-drift
bounds. The repository's complete Rust workspace, 584-case Birnam suite,
relocated production install, native-prefix, and two-snapshot cold-archive
gates passed. H6 intentionally still requires rerunning the complete driver on
the clean committed release archive with the selected prior-production
baseline; development-tree evidence is not promotable evidence.

## 10. Non-goals

60H does not add backward compatibility, another platform, public continuation
syntax, scheduling, shared-Runtime parallelism, an interpreter fallback,
automatic source recovery, or a new compilation-policy language surface.
