--------------------------- MODULE LiveSnapshot ---------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "old-code", "adopt-drift", "ignore-stamp", "replay", "undo-callback"}
VARIABLES phase, code, revision, eligible, epoch, observed, selected, file, fileCurrent, entered, changed, calls
vars == <<phase, code, revision, eligible, epoch, observed, selected, file, fileCurrent, entered, changed, calls>>
Init == /\ phase = "capture" /\ code = 2 /\ revision \in {1, 2} /\ eligible \in BOOLEAN /\ epoch = 1
        /\ observed = 0 /\ selected = 0 /\ file = 0 /\ fileCurrent = FALSE
        /\ entered = 0 /\ changed = FALSE /\ calls = 0
Capture == /\ phase = "capture" /\ phase' = "emit" /\ observed' = epoch
           /\ selected' = IF Bug = "old-code" THEN revision ELSE code
           /\ UNCHANGED <<code, revision, eligible, epoch, file, fileCurrent, entered, changed, calls>>
Emit == /\ phase = "emit" /\ phase' = "finish" /\ changed' \in BOOLEAN
        /\ code' = IF changed' THEN 3 ELSE code
        /\ epoch' = IF changed' THEN epoch + 1 ELSE epoch
        /\ calls' = IF Bug = "replay" THEN 2 ELSE 1
        /\ UNCHANGED <<revision, eligible, observed, selected, file, fileCurrent, entered>>
Finish == /\ phase = "finish" /\ phase' = "restored"
          /\ file' = IF epoch = observed \/ Bug = "ignore-stamp" THEN selected ELSE file
          /\ fileCurrent' = IF epoch = observed THEN ((eligible /\ selected = revision) \/ Bug = "adopt-drift") ELSE fileCurrent
          /\ entered' = IF epoch = observed THEN selected ELSE 0
          /\ code' = IF Bug = "undo-callback" THEN 2 ELSE code
          /\ UNCHANGED <<revision, eligible, epoch, observed, selected, changed, calls>>
Next == Capture \/ Emit \/ Finish
Spec == Init /\ [][Next]_vars
LiveCodeRestored == phase = "restored" /\ ~changed => entered = 2
DriftNotAdopted == fileCurrent => eligible /\ file = revision
ChangedCaptureNotPublished == phase = "restored" /\ changed => file = 0
CallbackEditSurvives == changed => code = 3
NoReplay == calls <= 1
=============================================================================
