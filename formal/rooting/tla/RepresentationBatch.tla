--------------------- MODULE RepresentationBatch ---------------------
EXTENDS Naturals, Sequences
CONSTANT Bug
ASSUME Bug \in {"none","second","services","own","delay","order"}
VARIABLES services, own, single, shapes, phase, grant, used, fallback, observed, legacyLeft
vars == <<services,own,single,shapes,phase,grant,used,fallback,observed,legacyLeft>>
Init == /\ services \in BOOLEAN /\ own \in BOOLEAN /\ single \in BOOLEAN
        /\ shapes \in [1..2 -> BOOLEAN] /\ phase = "request"
        /\ grant = FALSE /\ used = FALSE /\ fallback = FALSE
        /\ observed = <<>> /\ legacyLeft \in BOOLEAN
Current == services /\ own /\ single
Admitted == (services \/ Bug = "services") /\ (own \/ Bug = "own") /\ single
AllShapes == shapes[1] /\ (shapes[2] \/ Bug = "second")
Request == /\ phase = "request" /\ phase' = IF Admitted THEN "copy" ELSE "fallback"
           /\ grant' = Admitted /\ AllShapes /\ fallback' = ~Admitted
           /\ UNCHANGED <<services,own,single,shapes,used,observed,legacyLeft>>
Copy == /\ phase = "copy" /\ phase' = "done" /\ used' = grant
        /\ UNCHANGED <<services,own,single,shapes,grant,fallback,observed,legacyLeft>>
(* The actual owner reads every certified input before the next callback. *)
Delay == /\ phase = "copy" /\ Bug = "delay" /\ grant
         /\ shapes' = [shapes EXCEPT ![2] = FALSE]
         /\ UNCHANGED <<services,own,single,phase,grant,used,fallback,observed,legacyLeft>>
Fallback == /\ phase = "fallback" /\ phase' = "done"
            /\ observed' = IF Bug = "order" THEN <<"right","left">>
                            ELSE IF legacyLeft THEN <<"left","right">> ELSE <<"left">>
            /\ UNCHANGED <<services,own,single,shapes,grant,used,fallback,legacyLeft>>
Next == Request \/ Copy \/ Delay \/ Fallback
Spec == Init /\ [][Next]_vars
CertifiedBatch == used => Current /\ shapes[1] /\ shapes[2]
OriginalOrder == phase = "done" /\ fallback =>
                   observed = (IF legacyLeft THEN <<"left","right">> ELSE <<"left">>)
======================================================================
