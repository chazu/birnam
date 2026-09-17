# 80T — First cleanup-control implementation and evidence

**2026-09-16.** Implements the first control-mechanism slice of
[80N](80n-cancellation-to-cleanup-transition.md), following the accepted
[third review](80s-cleanup-control-review-synthesis.md). Starting source revision:
`1e45bdbb3c735fa86d038fb99a43cfe0b8c197c8`. The candidate is the working-tree
change above that revision; it is not a released or installed user version.

## Implemented

- Distinct `BoundaryCallAction` and `ExitBoundaryAction` families, ordinary checked
  Machine publishers, and reflective accessors. Existing action layouts and
  meanings are unchanged, including Abort.
- The driver establishes the retained handler before resolving/entering the body.
  Protected calls retain physical frames instead of using the optional inline-call
  path. An exit targets the nearest active boundary's exact descriptor V2 Call.
- Exit preflights owner, native barriers, pending edge, frame generations, delivery
  counts, and entry-pin counts before mutation. Its commit abandons body frames,
  commits only the handler's Call, and releases abandoned pins while retaining
  generation leases across the transfer.
- Boundary and delivery metadata belongs to frames. Capture/restore moves it with
  those frames; external retired Frame handles remain stale. Delivery survives
  normal returns and tail replacement until the library acknowledges the handoff.
- Image preflight explicitly rejects both new action families, including private
  derived vtables. Their canonical prototypes remain serializable references.

There is no public IdApi/Method ABI change, descriptor grammar change, or image
version change. The new state is transient runtime metadata, not image content.

## Focused execution evidence

`cargo test -p birnam-id boundary_control -- --nocapture` passes five tests in
both debug and release (`--release`) builds:

1. Cancellation/normal return through compiler-produced body frames, each using
   ordinary suspension and capture/restore. Cleanup executes once and waits again.
2. A compiler-produced caller publishes the new protected Call through an ordinary
   Machine method override. Cancellation after capture preserves its V2 result
   transfer and its delivery guard through a tail call into the waiting handler.
   Replacing the body's dispatch binding while captured does not replace the
   entered implementation or release its generation pin early.
3. Generation exhaustion, invalid pending edge, synchronous native barrier, nested
   entry barrier, and invalid pin ownership reject with execution intact.
4. Successful exit retires body handles and requires explicit delivery acknowledgement;
   wrong-owner and repeated/absent-boundary operations reject.
5. Delivery bookkeeping survives ordinary return and tail replacement, including
   more than one pending handoff.

The runtime traces force precise GC with native-stack scanning disabled while
body/handler execution is captured and while cleanup is waiting. They verify
fresh cleanup dispatch after method replacement, retained result values,
and zero active/continuation frame pins after completion.

These tests use fixture StepMethods for adapters/handlers and actual compiler
output for protected callers/bodies. They prove the control mechanism; they do
not establish a supplied Birnam task/ensure library or scheduler cancellation
semantics. The cancellation payload is an ordinary Oop chosen by the fixture.

## Compatibility evidence

Local raw logs, image inputs, artifact hashes, and command records are under
`.cargo-target/cleanup-control-evidence/`. The existing validation controller's
logs/receipts remain under `.cargo-target/validation/`.

| Check | Result / scope |
| --- | --- |
| Frozen IdApi client/module | Passed without rebuilding either artifact; all committed checksums match. |
| Archived image v4 | Rejected in mapped and rebuilt modes with the existing version mismatch. |
| Supported pre-change image | Image saved by the pre-change installed runtime loads in both mapped/rebuilt modes and its saved method returns 42. `baseline.json` records installed binary/library and image hashes; the installed source commit is not independently known. |
| Current-format image admission | Canonical action references round-trip in mapped/rebuilt modes; live instances reject before image publication. Extended `portable-image-v8-smoke.sh` also checks instances with private methods/vtables. |
| Export/dependency audit | `bash scripts/rust-dylib-audit.sh` passed for debug/release, including the unchanged public export manifest and install names. |
| Workspace and independent bytes | Workspace tests and frozen stackless/portable-code byte verifiers passed; final exhaustive run status is recorded below. |
| Full runtime regression workflow | Passed. The final `mise run verify-exhaustive` exited 0 with every phase and the complete-matrix/evidence audits passed. It reused matching, audited receipts from this candidate. |

The initial serial exhaustive attempt reached its 30-minute deadline after 28
phases passed. The remaining independent phases were completed concurrently using
the existing plan, PlanRunner, commands, input bindings, and coverage/audit oracles;
coverage was not reduced. A local runner under the ignored evidence directory
handled orchestration. The normal validator then rechecked and accepted the full
set of receipts. This adds no release procedure or manual release steps.

The final entry-point log is `.cargo-target/cleanup-control-exhaustive-final.log`;
the authoritative result is `.cargo-target/validation/run.json`. `candidate.json`
under the local evidence directory records the candidate artifact and changed
source hashes. The focused release log is `boundary-control-release.log` there.

The frozen fixture wrapper had a pre-existing stale expectation of `LoadRejected`.
Both the pre-change installed runtime and candidate report
`VersionMismatch { expected: 8, observed: 4 }`. The wrapper now checks that exact
diagnostic. Frozen binaries, image bytes, and checksum manifests were not changed.

The recovery authority check also froze the entire image admission source file.
Its comparison now permits exactly the two new action-name declarations and two
rejection-list entries. Every other byte remains compared against the existing
baseline; the image serializer and v8 format have no exception. Four tests in
`scripts/compatibility-authority-test.py` pass, including rejection of partial
additions, removal of an old rejection, and unrelated changes. No frozen baseline
was refreshed to make this change pass.

An additional run of the older `scripts/install-smoke.sh` stopped at its stale
workspace-package inventory, which omits `birnam-native-compiler`. That script was
left unchanged; new admission checks are integrated into the active portable-image
suite. This failure is not reported as a successful install-smoke run.

## Remaining work

The next slice was the supplied Birnam boundary/task/ensure protocol: install shielding before
acknowledging delivery, record genuine failure in traced task-owned state before
unmasking, preserve it through late cancellation, and test nested cleanup/child
settlement and stale wait notifications. That work and structured ownership are
now implemented and documented in [80U](80u-birnam-task-cleanup-slice.md), with
separate evidence for the expanded candidate. No existing `ensure:` implementation
was silently given new semantics by either slice.

CPU timeslicing/owner-side stopped-Machine exit, asynchronous cancellation
ingress, channel/domain APIs, and parallel execution remain later Design 80 work.
Independent cold-source release acceptance (`mise run verify-release`) and an
updated production installation have not been performed for this slice.
