------------------------- MODULE ModuleRegistry -------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none", "unprotected", "wrong-field", "abort-leak"}
Tasks == {1,2}
VARIABLES phase, registry, snapshot, owner
vars == <<phase, registry, snapshot, owner>>
Init == /\ phase = [t \in Tasks |-> "ready"]
        /\ registry = [f \in Tasks |-> FALSE]
        /\ snapshot = [t \in Tasks |-> [f \in Tasks |-> FALSE]]
        /\ owner = 0
Enter(t) == /\ phase[t] = "ready"
            /\ (owner = 0 \/ Bug = "unprotected")
            /\ owner' = IF Bug = "unprotected" THEN 0 ELSE t
            /\ phase' = [phase EXCEPT ![t] = "read"]
            /\ UNCHANGED <<registry, snapshot>>
Read(t) == /\ phase[t] = "read"
           /\ snapshot' = [snapshot EXCEPT ![t] = registry]
           /\ phase' = [phase EXCEPT ![t] = "publish"]
           /\ UNCHANGED <<registry, owner>>
Publish(t) == /\ phase[t] = "publish"
              /\ registry' = [snapshot[t] EXCEPT ![IF Bug = "wrong-field" THEN 1 ELSE t] = TRUE]
              /\ phase' = [phase EXCEPT ![t] = "done"]
              /\ owner' = 0
              /\ UNCHANGED snapshot
(* Map failure before publication leaves the old registry intact. *)
Abort(t) == /\ phase[t] \in {"read", "publish"}
            /\ phase' = [phase EXCEPT ![t] = "aborted"]
            /\ owner' = IF Bug = "abort-leak" THEN owner ELSE 0
            /\ UNCHANGED <<registry, snapshot>>
Next == \E t \in Tasks : Enter(t) \/ Read(t) \/ Publish(t) \/ Abort(t)
Spec == Init /\ [][Next]_vars
CompletedEditsSurvive == \A t \in Tasks : phase[t] = "done" => registry[t]
NoStrandedTransition == owner # 0 => phase[owner] \in {"read", "publish"}
=======================================================================
