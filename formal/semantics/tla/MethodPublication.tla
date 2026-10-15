------------------------- MODULE MethodPublication -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "rollback", "early-notify", "error-veto", "unrooted", "root-leak"}
VARIABLES phase, fault, dictionary, mirror, committed, notified, result,
          registered, pinned, alive
vars == <<phase, fault, dictionary, mirror, committed, notified, result,
          registered, pinned, alive>>
Init == /\ phase = "authorize"
        /\ fault \in {"none", "deny", "auth-fault", "raw-fault", "managed-fault", "error-value"}
        /\ dictionary = "old" /\ mirror = "old" /\ committed = FALSE
        /\ notified = FALSE /\ result = "pending"
        /\ registered = TRUE /\ pinned = FALSE /\ alive = TRUE
Authorize == /\ phase = "authorize"
             /\ phase' = IF fault \in {"deny", "auth-fault"} THEN "done" ELSE "commit"
             /\ result' = IF fault \in {"deny", "auth-fault"} THEN "veto" ELSE result
             /\ UNCHANGED <<fault, dictionary, mirror, committed, notified, registered, pinned, alive>>
Commit == /\ phase = "commit" /\ dictionary' = "new" /\ mirror' = "absent"
          /\ committed' = TRUE /\ phase' = "raw"
          /\ pinned' = (Bug # "unrooted")
          /\ UNCHANGED <<fault, notified, result, registered, alive>>
(* An entered observer clears its registration; collection follows. *)
Clear == /\ phase = "raw" /\ registered /\ registered' = FALSE
         /\ UNCHANGED <<phase, fault, dictionary, mirror, committed, notified, result, pinned, alive>>
Collect == /\ phase = "raw" /\ ~registered /\ alive' = pinned
           /\ phase' = "raw-return"
           /\ UNCHANGED <<fault, dictionary, mirror, committed, notified, result, registered, pinned>>
RawReturn == /\ phase = "raw-return"
             /\ phase' = IF fault = "raw-fault" THEN "done" ELSE "project"
             /\ result' = IF fault = "raw-fault" THEN "failure" ELSE result
             /\ dictionary' = IF fault = "raw-fault" /\ Bug = "rollback" THEN "old" ELSE dictionary
             /\ pinned' = (Bug = "root-leak")
             /\ UNCHANGED <<fault, mirror, committed, notified, registered, alive>>
Project == /\ phase = "project"
           /\ mirror' = IF Bug = "early-notify" THEN mirror ELSE dictionary
           /\ phase' = "managed" /\ notified' = TRUE
           /\ UNCHANGED <<fault, dictionary, committed, result, registered, pinned, alive>>
Managed == /\ phase = "managed" /\ phase' = "done"
           /\ result' = IF fault = "managed-fault" THEN "failure"
                         ELSE IF fault = "error-value" /\ Bug = "error-veto" THEN "veto" ELSE "ok"
           /\ UNCHANGED <<fault, dictionary, mirror, committed, notified, registered, pinned, alive>>
Next == Authorize \/ Commit \/ Clear \/ Collect \/ RawReturn \/ Project \/ Managed
Spec == Init /\ [][Next]_vars
CommitSurvives == committed => dictionary = "new"
AuthorizationFirst == fault \in {"deny", "auth-fault"} => ~committed
MirrorBeforeManaged == notified => mirror = dictionary
VoidErrorIsNotVeto == phase = "done" /\ fault = "error-value" => result = "ok"
EnteredReceiverAlive == phase = "raw-return" => alive
TemporaryRootsReleased == phase = "done" => ~pinned
=======================================================================
