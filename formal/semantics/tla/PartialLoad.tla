-------------------------- MODULE PartialLoad --------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "undo", "authority", "replay"}
VARIABLES phase, value, applied, prepared, composition, callbacks
vars == <<phase, value, applied, prepared, composition, callbacks>>
Init == /\ phase = "apply" /\ value = 0 /\ applied = 0
        /\ prepared = TRUE /\ composition = TRUE /\ callbacks = 0
Apply == /\ phase = "apply" /\ phase' = "callback"
         /\ value' = 1 /\ applied' = 1
         /\ UNCHANGED <<prepared, composition, callbacks>>
Callback == /\ phase = "callback" /\ phase' = "fail"
            /\ value' \in {1, 2} /\ applied' = value' /\ callbacks' = 1
            /\ UNCHANGED <<prepared, composition>>
Fail == /\ phase = "fail" /\ phase' = "done"
        /\ value' = IF Bug = "undo" THEN 0 ELSE value
        /\ prepared' = (Bug = "authority") /\ composition' = FALSE
        /\ callbacks' = IF Bug = "replay" THEN 2 ELSE callbacks
        /\ UNCHANGED applied
Next == Apply \/ Callback \/ Fail
Spec == Init /\ [][Next]_vars
AppliedChangesRemain == phase = "done" => value = applied
FailedAuthorityRevoked == phase = "done" => ~prepared /\ ~composition
NoReplay == callbacks <= 1
=============================================================================
