# 71 — PEG matching allocation

Status: rule indexing and once-per-parse pattern ownership implemented and
correctness-verified. Ticket: `birnam-peg-matching-plan-ki3` (implementation
authorized after planning).

## Evidence

`match_pattern` clones literal bytes, child pointers and charset ranges at every
visit, even though matching makes no language sends. `named_rule` linearly scans
the copied rule vector before checking memoization. Memo keys are pattern object
identity plus input position, not rule-name identity. Successful rule memo entries
copy a one-element child vector. CST conversion has separate allocations and
rooting; it must not be conflated with matching cost.

`bench/peg-matching-investigation.birnam` builds a repeated one-byte atom rule
after unused rules and times full parsing/CST conversion, excluding grammar and
input construction. One fresh release-process diagnostic, microseconds:

| Unused rules before atom | 16 bytes | 1,024 bytes | 8,192 bytes |
| ---: | ---: | ---: | ---: |
| 0 | 10 | 218 | 2,256 |
| 63 | 14 | 433 | 2,281 |
| 255 | 18 | 577 | 2,714 |

Every result had the requested end offset and child count. Redefining atom from
`x` to `y` after parsing caused `x` to fail and `y` to parse at all three sizes
of grammar. The input traverses the atom rule N+1 times (including terminal
failure), so name comparisons scale as (N+1)*(unused+1), plus the root lookup.
The literal is also visited N+1 times and currently copies its byte Vec each
time. These operation counts are source-derived, not allocation counters.

This small diagnostic does not establish name lookup as the dominant cost:
CST creation and matching have substantial fixed/linear costs, and timings
are not host-gated. Profile the JSON grammar separately before claiming a
workload speedup. Preserve the existing `parse.json-20kb` benchmark unchanged.

## Selected design

Keep the mutable Grammar/Pattern interface and transient CST contract of Design
63. Use private parse-local state, never a persistent compiled-grammar cache.

1. Build a rule-name identity lookup table for the existing parse snapshot;
   preserve first-match behavior and resolve root/rule references with it.
   Preserve the existing memo key (pattern identity, position), including aliases.
2. Snapshot each distinct pattern's literal/children/ranges at most once per
   parse into a private indexed graph. Matching reads that graph without cloning
   vectors per visit. Maintain a pattern-identity-to-index table so shared
   patterns stay shared. Separate immutable pattern data from mutable memo/node
   scratch to make Rust borrowing local and explicit.
3. Prefer lazy snapshotting of visited patterns, or a reachable graph snapshot,
   over copying every unused rule graph on small inputs. Measure eager versus
   lazy overhead before choosing; the first rule-index slice stands on its own.

Do not eagerly reject undefined references in an unvisited ordered-choice arm:
that would change parse behavior. Indexing must not accidentally redefine
left-recursion detection, memo aliases or error precedence. Traverse snapshot
graphs with visited identities; do not add unchecked recursive graph copying.

Keep Oop owners rooted throughout snapshot use and CST materialization. No raw
borrow from a mutable Pattern/Grammar may cross a language send or GC-capable
conversion call. Per-parse data is discarded afterwards; a mutation before the
next parse must be visible. Memo child-vector compression is a separate slice,
not a prerequisite for eliminating per-visit pattern clones.

## Bounded implementation and validation

1. Pin the current matcher as a differential oracle. Generate valid small
   grammars and compare success/failure, full rule-labelled CST spans/order,
   furthest-failure offset and error kinds. Add shared-pattern aliases and
   unreachable undefined rules before altering representation.
2. Implement rule indexing alone; benchmark 0/63/255 unused rules and actual
   JSON. Test root replacement and mutation between parses.
3. Implement once-per-parse pattern ownership, with allocation counters for
   literals, sequences, charsets and shared patterns. Assert copies scale with
   visited pattern data, not visits. Test small-input overhead and peak memory.
4. Cover zero-width star/plus, lookahead with backtracking, undefined/recursive
   rules, per-Grammar diagnostics, deep acyclic rules and forced-GC CST conversion.
5. Run Rust PEG tests and current production `mise run verify` / `mise run test`,
   plus installed/source parsing parity. Design 63's old Zig commands are
   historical; use current Rust gates rather than reviving the retired floor.
6. Collect host-gated before/after `parse.json-20kb` and source-startup timings,
   keeping corpus, repetitions and release thresholds fixed. Accept only with
   correctness and bounded memory, not merely fewer apparent clone calls.

No grammar syntax, IdApi, image format, error behavior or persistent cache is
needed. The first implementation slice requires no user-facing semantic change.

## Probe provenance

All Birnam probes for Designs 69–71 used the fresh release prefix
`/tmp/birnam-planning.sipzqN/prefix`, built from the query-allocation runtime
at `d15de56d` plus the test-only Store diagnostic. Source-manifest SHA-256:
`7ce30f43edeafbdb9bc626f68c4a06b27d7c630c938a818366b746ff47107223`.
Logs are supplemental in `/tmp/birnam-planning.sipzqN/`; probe sources and
the observations above are retained in the repository. No production code
was optimized in this investigation sequence.

## Implementation

Each Matcher now has a first-wins rule-name identity index and a lazy
pattern-identity map of immutable owned PatternState snapshots. Rc handles
permit recursive matching to borrow immutable pattern data while mutating memo
and CST scratch; cloning a handle does not allocate or copy pattern vectors.
Only visited patterns are copied, once per parse. Memo keys, child-node carriers,
floor object layouts, CST conversion and public Grammar/Pattern methods remain
unchanged. No persistent compiled-grammar cache was introduced.

A frozen test-only matcher compares outcomes, diagnostics and every temporary
node, including discarded lookahead nodes, for 261 generated pattern roots over
eight inputs (2,088 comparisons). This includes all combinators, zero-width
repetition, undefined/recursive rules and alias names sharing a pattern. A
128-rule acyclic chain preserves spans and labels. The repeated-input test keeps
exactly three copied patterns, one literal byte and one child pointer for
1/100/1,000 bytes; an unused 4KB literal is never copied. New invocations observe
pattern mutation. These are pattern-copy/ownership checks, not total allocator
counts: memo and CST allocation is intentionally unchanged.

Validation: focused Rust PEG tests, full `mise run verify`, all 611 language
tests and runtime bounded-churn save/load/resave passed. The forced-source PEG
probe retained all end-offset, child-count and between-parse mutation checks.
The optimized runtime was built into a private release prefix; no user
installation or running runtime was replaced.

## Performance acceptance — closed 2026-09-09

The acceptance repeat ([record](../measurements/as7-acceptance-comparison.json);
procedure in [Design 68](68-store-snapshot-scaling.md)) reproduced the startup
regression and attributed all of it to this change: over 15 interleaved reps
the PEG-only build of `3bff9357` booted stdlib in 112.4 ms against 105.9 ms
before, while the Store-only build measured 105.0 ms.

Cause: a dylib boot parses every self-hosted floor source through the 26-rule
language grammar, about 250 KB per boot with each source parsed twice, and the
rule index, the pattern-snapshot table and the memo table hashed integer keys
with SipHash on every pattern visit. On the common leaf kinds that cost more
than the small vector clones it replaced.

Fix (`f24ff9b2`): the parse-local tables use an address/position hasher of the
same shape as `birnam-id`'s address map. The lazy snapshot, memo keys,
first-wins rule index, diagnostics and the frozen legacy differential matcher
are unchanged; the matcher tests and the 627-expect language suites pass.
Because the memo table shares the hasher, parsing is now faster than before
this design: gated `parse.json-20kb` 361.9/357.9 ms before to 310.2 ms fixed
(-14.2%); dylib boot 107.0 to 86.0 ms and docgen 39.6 to 29.4 s together with
Design 68 and the as7 index work. Dylib-startup peak RSS was 65.5 MB on every
run; the earlier +15.3% did not reproduce.

Observed but not addressed here: the double parse of each floor source on
every dylib boot, tracked as a separate investigation in Beads.
