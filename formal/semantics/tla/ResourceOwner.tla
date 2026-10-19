------------------------- MODULE ResourceOwner -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "retain", "erase", "stale"}
VARIABLES phase, owner, held, attempts, replaced, reused, newAlive
vars == <<phase,owner,held,attempts,replaced,reused,newAlive>>
Init == /\ phase = "observed" /\ owner = 1 /\ held = 0
        /\ attempts = 0 /\ replaced = FALSE /\ reused = FALSE /\ newAlive = FALSE
(* The observed record is 1. An outgoing getter can install record 2. *)
Before == /\ phase = "observed" /\ phase' = "claim"
          /\ \E change \in BOOLEAN:
               /\ owner' = (IF change THEN 2 ELSE owner)
               /\ replaced' = change
          /\ UNCHANGED <<held,attempts,reused,newAlive>>
Claim == /\ phase = "claim" /\ phase' = "callback"
         /\ held' = IF owner = 1 \/ Bug = "stale" THEN 1 ELSE 0
         /\ owner' = IF held' = 1 /\ Bug # "retain" THEN 0 ELSE owner
         /\ UNCHANGED <<attempts,replaced,reused,newAlive>>
Skip == /\ phase = "callback" /\ phase' = "finish"
        /\ UNCHANGED <<owner,held,attempts,replaced,reused,newAlive>>
Reenter == /\ phase = "callback" /\ ~replaced /\ phase' = "finish"
           /\ attempts' = attempts + IF owner = 1 THEN 1 ELSE 0
           /\ owner' = IF Bug = "retain" THEN owner ELSE 0
           /\ UNCHANGED <<held,replaced,reused,newAlive>>
Replace == /\ phase = "callback" /\ ~replaced /\ phase' = "finish"
           /\ owner' = 2 /\ replaced' = TRUE
           /\ UNCHANGED <<held,attempts,reused,newAlive>>
Finish == /\ phase = "finish" /\ phase' = "completed"
          /\ attempts' = attempts + IF held = 1 THEN 1 ELSE 0
          /\ owner' = IF Bug = "erase" THEN 0 ELSE owner
          /\ UNCHANGED <<held,replaced,reused,newAlive>>
(* A new connection receives the old numeric descriptor after the first closes. *)
Reuse == /\ phase = "completed" /\ ~replaced /\ phase' = "again"
         /\ reused' = TRUE /\ newAlive' = TRUE
         /\ UNCHANGED <<owner,held,attempts,replaced>>
Again == /\ phase = "again" /\ phase' = "done"
         /\ attempts' = attempts + IF owner = 1 THEN 1 ELSE 0
         /\ newAlive' = IF owner = 1 THEN FALSE ELSE newAlive
         /\ UNCHANGED <<owner,held,replaced,reused>>
Stop == /\ phase = "completed" /\ phase' = "done"
        /\ UNCHANGED <<owner,held,attempts,replaced,reused,newAlive>>
Next == Before \/ Claim \/ Skip \/ Reenter \/ Replace \/ Finish \/ Reuse \/ Again \/ Stop
Spec == Init /\ [][Next]_vars
AtMostOnce == attempts <= 1
ForeignSurvives == replaced => owner = 2
ReusedDescriptorSurvives == reused => newAlive
=======================================================================
