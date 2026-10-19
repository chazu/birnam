-------------------------- MODULE ImagePayload --------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none", "root", "identity", "native", "replay", "failure", "leak"}
VARIABLES phase, reply, family, scope, heap, calls, saved, identity
vars == <<phase,reply,family,scope,heap,calls,saved,identity>>
Init == /\ phase = "ready" /\ reply \in {"self","fresh","reject","error"}
        /\ family \in {"plain","native"} /\ scope = {} /\ heap = {1,2,3}
        /\ calls = 0 /\ saved = FALSE /\ identity = 0
(* 1 is the original receiver; fresh payload 2 contains fresh child 3. *)
Reachable == {1} \cup scope \cup (IF 2 \in scope THEN {3} ELSE {})
Admitted == reply = "self" \/ (reply = "fresh" /\ (family = "plain" \/ Bug = "native"))
Policy == /\ phase = "ready" /\ calls' = calls + 1 /\ phase' = "returned"
          /\ scope' = IF reply = "fresh" /\ Bug # "root" THEN {2} ELSE {}
          /\ heap' = IF reply = "fresh" THEN heap \cup {2,3} ELSE heap
          /\ UNCHANGED <<reply,family,saved,identity>>
Read == /\ phase = "returned" /\ phase' = "done"
        /\ saved' = (Admitted \/ Bug = "failure")
        /\ identity' = IF Admitted THEN (IF Bug = "identity" /\ reply = "fresh" THEN 2 ELSE 1) ELSE 0
        /\ scope' = IF Bug = "leak" THEN scope ELSE {}
        /\ UNCHANGED <<reply,family,heap,calls>>
Replay == /\ phase = "returned" /\ Bug = "replay" /\ calls = 1 /\ calls' = 2
          /\ UNCHANGED <<phase,reply,family,scope,heap,saved,identity>>
Collect == /\ heap' = heap \cap Reachable
           /\ UNCHANGED <<phase,reply,family,scope,calls,saved,identity>>
Next == Policy \/ Read \/ Replay \/ Collect
Spec == Init /\ [][Next]_vars
PayloadAlive == phase = "returned" /\ reply = "fresh" => {2,3} \subseteq heap
OriginalIdentity == saved => identity = 1
NativeRepresentation == saved /\ family = "native" => reply = "self"
Once == calls <= 1
FailurePreservesFile == reply \in {"reject","error"} => ~saved
ScopesDrain == phase = "done" => scope = {}
=======================================================================
