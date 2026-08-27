# 60HA — Adversarial Review: Stackless Production Acceptance and Rollout

> **Status:** Review complete 2026-08-27. All blocking corrections are
> incorporated into [60H](60h-stackless-production-acceptance-and-rollout.md).

## 1. “Remove readers” accidentally removes v7's fact grammar

**Attack:** v7 inherited canonical field framing and retention-aware facts from
earlier implementation modules. Deleting every symbol with a legacy version in
its name could duplicate the grammar or make v7 lose durable retention.

**Resolution:** 60H distinguishes a production admission reader from a helper
owned by current v7. Shared helpers move to version-neutral ownership before
legacy modules are deleted. There remains one current authority and no callable
v4/v5/v6 admission path. **Resolved.**

## 2. A version check after preflight is still a compatibility reader

**Attack:** Parsing all legacy structures and rejecting only before publication
retains a large attack and maintenance surface while claiming no compatibility.

**Resolution:** Image version rejects immediately after the fixed 28-byte
header check and before version-specific structural parsing. Candidate
manifests reject their prefix before old tokenization. Historical full parsers
are removed. **Resolved.**

## 3. Source rebuild silently changes requested disposition

**Attack:** An obsolete or incompatible artifact could be treated as a request
to recompile source under today's policy, changing synchronous/resumable
selection while appearing to load successfully.

**Resolution:** Runtime artifact admission rejects. Build tools may rebuild an
obsolete cache only from current source plus a newly materialized current
selection. V7 rebuild uses the exact selection stored in v7 and is therefore a
physical projection, not policy reselection. **Resolved.**

## 4. IdApi v1 looks like a backward-compatibility exception

**Attack:** Retaining a name ending in v1 appears inconsistent with an
exact-current policy.

**Resolution:** “v1” names the current ABI. Admission still requires the exact
current fingerprint. A future break changes the fingerprint/version and may
reject all old modules. No shim is promised. **Resolved.**

## 5. Release verification rebuilds different bytes

**Attack:** A pipeline may run tests from a checkout, then rebuild or resign a
different archive for publication. The receipt would attest to intent, not the
released program.

**Resolution:** H2/H3 run against one immutable archive. The receipt includes
the archive and member digests. Promotion copies those exact bytes and rejects
any digest drift. Signing must occur before verification or be separately
identified as an envelope over identical payload bytes. **Resolved.**

## 6. A receipt self-attests without preserving evidence

**Attack:** Recording `passed: true` is circular if the underlying command and
output are unavailable or can refer to another binary.

**Resolution:** Each gate records command, exit status, output digest, archive
digest, and relevant structured measurements. Complete logs are retained and
hashed. Receipt validation recomputes artifact identity. **Resolved.**

## 7. Twenty epochs merely repeat one process

**Attack:** A process-only loop misses image recovery leaks and a process-per-
epoch loop misses in-process age growth.

**Resolution:** Each epoch contains a substantial fixed-live-set in-process
batch, then saves and exits. The next epoch continues from that image. This
tests both lifetimes. Mapped and rebuilt recovery alternate. **Resolved.**

## 8. RSS equality is not portable or allocator-stable

**Attack:** Requiring byte-exact RSS makes the gate flaky because allocators and
the OS retain pages.

**Resolution:** Semantic counters are exact where the implementation exposes
exact ownership. RSS and allocator capacity use a declared bounded slack and a
no-age-slope rule after warm-up. The receipt preserves raw observations.
The first complete arm64 macOS calibration fixed a 192 MiB high-water spread
bound and a separate 64 MiB first-to-last age-drift bound after every exact
semantic counter and image size stayed flat while RSS varied by 144 MiB but
ended only 22 MiB above the first post-warm-up epoch.
**Resolved.**

## 9. The retained-history control can hide a real leak

**Attack:** Total growth could be excused as explicit retention without proving
which bytes are retained.

**Resolution:** The control runs separately, records Store retained facts and
bytes, predicts its growth, then returns to current-only and must compact back
to the fixed-live-set bound. Unattributed growth remains failure. **Resolved.**

## 10. A clean cut removes operational rollback

**Attack:** Without legacy readers or a downgrade writer, a bad release might
strand an image.

**Resolution:** Rollback is between verified exact-current releases and v7
checkpoints. 60H makes no promise for an operator who deletes every checkpoint.
This is explicit durable-artifact ownership, not an implicit hidden backup.
**Resolved.**

## 11. Performance baselines ratchet silently

**Attack:** A candidate can pass by replacing the previous baseline with its
own slower result.

**Resolution:** The previous published receipt is the baseline. A regression
outside budget requires a separate accepted design decision before the new
receipt can be published. **Resolved.**

## 12. Unsupported platforms accidentally inherit the production claim

**Attack:** Portable Rust tests may pass on another host while native loading,
TCC, signing, or stack-root behavior remains unproved.

**Resolution:** Production support remains arm64 macOS. A new target must pass
the entire packaged matrix before platform documentation changes. **Resolved.**

## 13. Residual risks

- The first complete longevity drive may expose operational cost or missing
  telemetry. That is an implementation finding, not permission to weaken the
  plateau equation.
- Release signing and external publication credentials remain deployment
  concerns. 60H proves byte identity up to the publication boundary and must
  not mutate external release state without explicit authorization.
- Exact-current policy intentionally makes old durable artifacts unusable.
  This is the accepted product decision, not a migration defect.

## 14. Conclusion

60H is implementable without reopening stackless execution semantics. Its hard
requirements are the early exact-version cut, one immutable candidate, complete
evidence provenance, and the combined process/image lifetime plateau.
