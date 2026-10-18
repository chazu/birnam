------------------------- MODULE PredecessorRelease -------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES phase, handle, status, observed, binding
vars == <<phase, handle, status, observed, binding>>
Init == /\ phase = "ready" /\ handle = TRUE /\ status = "superseded"
        /\ observed = "none" /\ binding = 2
Discard == /\ handle /\ handle' = FALSE /\ status' = "discarded"
           /\ UNCHANGED <<phase, observed, binding>>
Prepare == /\ phase = "ready" /\ phase' = "prepared" /\ observed' = status
           /\ UNCHANGED <<handle, status, binding>>
Commit == /\ phase = "prepared" /\ phase' = "done"
          /\ binding' = IF Bug = "stale" /\ observed # status THEN binding ELSE 1
          /\ status' = IF handle \/ Bug = "revive" THEN "active" ELSE status
          /\ UNCHANGED <<handle, observed>>
Next == Discard \/ Prepare \/ Commit
Spec == Init /\ [][Next]_vars
ReleasedStatusPreserved == ~handle => status = "discarded"
UndoIndependentOfHandle == phase = "done" => binding = 1
=============================================================================
