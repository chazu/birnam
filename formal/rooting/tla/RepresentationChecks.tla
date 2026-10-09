------------------------ MODULE RepresentationChecks -----------------
EXTENDS Naturals, Sequences
CONSTANT Bug
ASSUME Bug \in {"none","getter","own","cached","delay","order"}
VARIABLES phase, round, getters, own, shape, grant, used, route, legacy, observed
vars == <<phase,round,getters,own,shape,grant,used,route,legacy,observed>>
Init == /\ phase = "request" /\ round = 0
        /\ getters \in BOOLEAN /\ own \in BOOLEAN /\ shape \in BOOLEAN
        /\ grant = FALSE /\ used = FALSE /\ route = "none"
        /\ legacy \in {<<"behavior","size">>,<<"identity","behavior","size">>}
        /\ observed = <<>>
Admitted == (getters /\ own) \/ (Bug = "getter" /\ own) \/ (Bug = "own" /\ getters)
            \/ (Bug = "cached" /\ round = 1 /\ grant)
Request == /\ phase = "request"
           /\ phase' = IF Admitted THEN "read" ELSE "fallback"
           /\ grant' = (Admitted /\ shape)
           /\ route' = IF Admitted THEN "atomic" ELSE "fallback"
           /\ UNCHANGED <<round,getters,own,shape,used,legacy,observed>>
Read == /\ phase = "read" /\ phase' = "done" /\ used' = grant
        /\ UNCHANGED <<round,getters,own,shape,grant,route,legacy,observed>>
Fallback == /\ phase = "fallback" /\ phase' = "done"
            /\ observed' = IF Bug = "order" THEN <<"size","behavior">> ELSE legacy
            /\ UNCHANGED <<round,getters,own,shape,grant,used,route,legacy>>
(* No callbacks are allowed between certification and the owner's raw read. *)
Delay == /\ Bug = "delay" /\ phase = "read" /\ grant
         /\ getters' = FALSE /\ shape' = FALSE
         /\ UNCHANGED <<phase,round,own,grant,used,route,legacy,observed>>
NextCall == /\ phase = "done" /\ round = 0 /\ grant
            /\ phase' = "request" /\ round' = 1 /\ getters' = FALSE
            /\ used' = FALSE /\ route' = "none" /\ observed' = <<>>
            /\ UNCHANGED <<own,shape,grant,legacy>>
Next == Request \/ Read \/ Fallback \/ Delay \/ NextCall
Spec == Init /\ [][Next]_vars
CertifiedRead == used => (getters /\ own /\ shape)
OriginalFallbackOrder == phase = "done" /\ route = "fallback" => observed = legacy
======================================================================
