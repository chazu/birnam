------------------------- MODULE CandidateUndo -------------------------
EXTENDS Naturals
CONSTANT Bug
VARIABLES phase, value, applied, expected, undo, foreign, held, mapped, calls
vars == <<phase, value, applied, expected, undo, foreign, held, mapped, calls>>
Init == /\ phase = "prepared" /\ value = 0 /\ applied = 0 /\ expected = 1
        /\ undo = FALSE /\ foreign = FALSE /\ held \in BOOLEAN /\ mapped = TRUE /\ calls = 0
Write == /\ phase = "prepared" /\ phase' = "callback" /\ value' \in {0, 1}
         /\ undo' = TRUE /\ calls' = 1
         /\ UNCHANGED <<applied, expected, foreign, held, mapped>>
Callback == /\ phase = "callback" /\ phase' = "fail" /\ foreign' \in BOOLEAN
            /\ value' = (IF foreign' THEN 2 ELSE value) /\ applied' = value'
            /\ expected' = IF Bug = "learn" THEN value' ELSE expected
            /\ UNCHANGED <<undo, held, mapped, calls>>
Failure == /\ phase = "fail" /\ phase' = "failed"
           /\ value' = IF Bug = "automatic" THEN 0 ELSE value
           /\ undo' = (Bug # "forget")
           /\ mapped' = IF Bug = "code" THEN FALSE ELSE mapped
           /\ calls' = IF Bug = "replay" THEN 2 ELSE calls
           /\ UNCHANGED <<applied, expected, foreign, held>>
Undo == /\ phase = "failed" /\ phase' = "undone"
        /\ value' = IF value = 0 \/ value = expected THEN 0 ELSE value
        /\ undo' = (value # 0 /\ value # expected)
        /\ UNCHANGED <<applied, expected, foreign, held, mapped, calls>>
Next == Write \/ Callback \/ Failure \/ Undo
Spec == Init /\ [][Next]_vars
FailureRetainsEffects == phase = "failed" => value = applied
FailureRetainsUndo == phase = "failed" => undo
UndoPreservesForeign == phase = "undone" /\ foreign => value = 2 /\ undo
HeldCodeLives == held => mapped
NoReplay == calls <= 1
=============================================================================
