# 70 — Persistent Map deletion

Status: investigation and proposed plan recorded, 2026-09-05; equality/identity
approval pending. Ticket: `birnam-map-deletion-plan-iis`. No production changes.

## Reproduction and cost

`bench/map-deletion-investigation.birnam` runs against a fresh release prefix.
Two separately allocated `#[1 2]` keys answer false to `==`, true to `=`, and
coexist in a Map of size two with independently retrievable values. Calling
`removeKey:` with the first key produces a Map of size zero: both disappear.
The original Map remains size two. Removing an unrelated key returns an equal
Map of size two but not the identical Map.

This follows the implementation: source-authored removal enumerates keys and
uses ordinary `=`, while native HAMT lookup/insertion uses `map_key_equal` and
`map_key_hash`. Native key equality compares numbers/strings by the existing
scalar policy and other objects by identity. Vector `=` is structural.

Absent integer-key removal from 16/64/256-entry Maps took 367/1,264/4,900 us in
one diagnostic run with default GC. It preserves size but returns a different
Map at every size. These are not host-gated performance results. Regardless of
timing, the implementation visits every key, looks up each surviving value and
inserts it into a new persistent HAMT: roughly O(n * trie-depth), with O(n)
language callbacks and repeated path allocations even for misses.

## Decision required

Recommended: `removeKey:` uses precisely lookup/insertion key equivalence and
hashing, and absent-key removal may return the original receiver. The user has
been asked to approve both changes. This deliberately changes the equal-Vector
case and observable identity on misses, and removes ordinary equality callbacks
from deletion. It does not redefine all Map keys to use structural equality.

Preserving arbitrary user `=` effects requires enumeration and prevents the
intended keyed deletion. A separate structural-filter operation would be a
different feature, not an implicit compatibility path in HAMT deletion.

## Plan after approval

1. Turn the reproduction into desired-behavior regression tests. Pin distinct
   equal Vector keys, identical keys, String/numeric policy, nil values, absent
   keys, original-version persistence and ordering of remaining keys/values.
2. Add a private native HAMT deletion returning replacement node plus a local
   removed flag. Do not add a global deletion-status variable. Follow the
   existing hash path; copy only affected ancestors and share untouched nodes.
3. Handle bitmap leaves, empty child removal and collision buckets explicitly.
   A collision scan is O(bucket size); do not claim unconditional constant time.
   Preserve hash-depth interpretation: collapse an internal child only when its
   representation can be safely promoted, not merely because it has one child.
   Leave redundant-but-correct internal shape initially if collapse is risky.
4. Preserve rooted source, replacement nodes and result across GC-capable
   allocation; no user callbacks or Rust references span such calls. Test
   forced GC through nested paths and collision deletion, then image round trips.
5. Install the native primitive and adjust/remove the stdlib definition that
   would otherwise overwrite it at bootstrap. Verify both source and AOT
   installation, ordinary dispatch replacement and documentation. No IdApi growth.
6. Compare hit/miss/delete-last at 0/1/16/64/256/4096 entries, collision-heavy
   cases and repeated deletion while retaining old versions. Count node copies
   and allocations in addition to timings. Misses should require no new Map or
   HAMT node. Changes must pass collection tests, `mise run verify`,
   `mise run test`, image and bounded-churn gates.

Keep key policy changes distinct from data-structure mechanics in review, even
if the native implementation ships them together. The current reproduction is
a characterization probe; its output is expected to change after approval.
