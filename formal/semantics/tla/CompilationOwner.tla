-------------------------- MODULE CompilationOwner --------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "stamp", "alias", "value", "facts"}
VARIABLES selected, revision, token, snapshot, captured, phase, edits, facts
vars == <<selected, revision, token, snapshot, captured, phase, edits, facts>>
Init == /\ selected = 0 /\ revision = 0 /\ token = 0 /\ snapshot = 0
        /\ captured = 0 /\ phase = "prepare" /\ edits = 0 /\ facts = 0
Prepare == /\ phase = "prepare" /\ phase' = "ready"
           /\ token' = revision /\ snapshot' = selected /\ captured' = selected
           /\ facts' = IF Bug = "facts" THEN 1 ELSE facts
           /\ UNCHANGED <<selected, revision, edits>>
Replace == /\ phase = "ready" /\ edits < 2
           /\ selected' = 1 - selected /\ edits' = edits + 1
           /\ revision' = IF Bug = "stamp" THEN revision ELSE revision + 1
           /\ snapshot' = IF Bug = "alias" THEN selected' ELSE snapshot
           /\ UNCHANGED <<token, captured, phase, facts>>
Commit == /\ phase = "ready"
          /\ phase' = IF (IF Bug = "value" THEN selected = snapshot ELSE token = revision)
                       THEN "committed" ELSE "rejected"
          /\ UNCHANGED <<selected, revision, token, snapshot, captured, edits, facts>>
Next == Prepare \/ Replace \/ Commit
Spec == Init /\ [][Next]_vars
SnapshotStable == snapshot = captured
NoImplicitFacts == facts = 0
NoStaleCommit == phase = "committed" => edits = 0
NoSpuriousInvalidation == phase = "rejected" => edits > 0
=============================================================================
