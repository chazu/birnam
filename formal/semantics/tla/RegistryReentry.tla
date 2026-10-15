------------------------- MODULE RegistryReentry -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "overwrite", "swallow", "replay"}
VARIABLES phase, root, snapshot, foreign, outer, callbacks, result
vars == <<phase, root, snapshot, foreign, outer, callbacks, result>>
Init == /\ phase = "read" /\ root = 0 /\ snapshot = 0
        /\ foreign = FALSE /\ outer = FALSE /\ callbacks = 0 /\ result = "pending"
Read == /\ phase = "read" /\ snapshot' = root /\ phase' = "construct"
        /\ UNCHANGED <<root, foreign, outer, callbacks, result>>
(* Synchronous callback runs inside a task transition, not a second task. *)
Callback == /\ phase = "construct" /\ callbacks = 0
            /\ root' = 1 /\ foreign' = TRUE /\ callbacks' = 1
            /\ UNCHANGED <<phase, snapshot, outer, result>>
Construct == /\ phase = "construct" /\ phase' = "publish"
             /\ UNCHANGED <<root, snapshot, foreign, outer, callbacks, result>>
Publish == /\ phase = "publish" /\ phase' = "done"
           /\ IF root = snapshot \/ Bug = "overwrite"
                 THEN /\ root' = 2 /\ outer' = TRUE /\ foreign' = FALSE /\ result' = "ok"
                 ELSE /\ UNCHANGED <<root, outer, foreign>>
                      /\ result' = IF Bug = "swallow" THEN "ok" ELSE "conflict"
           /\ callbacks' = IF Bug = "replay" /\ root # snapshot THEN callbacks + 1 ELSE callbacks
           /\ UNCHANGED snapshot
Next == Read \/ Callback \/ Construct \/ Publish
Spec == Init /\ [][Next]_vars
ForeignSurvives == callbacks > 0 => foreign
ConflictReported == phase = "done" /\ callbacks > 0 => result = "conflict" /\ ~outer
NoReplay == callbacks <= 1
=======================================================================
