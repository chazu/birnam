----------------------- MODULE HelperEmission -----------------------
EXTENDS Naturals, Sequences
CONSTANT Bug
ASSUME Bug \in {"none","transitive","address","dispatch"}
(* 1 exported entry, 2 direct helper, 3 transitive helper, 4 address-taken
   callback, 5 unused helper. Calls through 3 and 4 use the CURRENT world. *)
VARIABLES pc, world, changed, retained, trace, expected
vars == <<pc,world,changed,retained,trace,expected>>
Plan == <<1,2,3,4>>
Init == /\ pc = 1 /\ world = 0 /\ changed = FALSE
        /\ retained = CASE Bug = "transitive" -> {1,2,4}
                        [] Bug = "address" -> {1,2,3}
                        [] OTHER -> {1,2,3,4}
        /\ trace = <<>> /\ expected = <<>>
Replace == /\ ~changed /\ world' = 1 /\ changed' = TRUE
           /\ UNCHANGED <<pc,retained,trace,expected>>
Call == /\ pc <= Len(Plan) /\ pc' = pc+1
        /\ trace' = Append(trace,IF Plan[pc] \in retained
             THEN <<Plan[pc],IF Bug = "dispatch" THEN 0 ELSE world>> ELSE <<0,0>>)
        /\ expected' = Append(expected,<<Plan[pc],world>>)
        /\ UNCHANGED <<world,changed,retained>>
Next == Replace \/ Call
Spec == Init /\ [][Next]_vars
SameCalls == trace = expected
=====================================================================
