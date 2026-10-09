-------------------------- MODULE BulkCaptures -----------------------
EXTENDS Naturals, FiniteSets, Sequences
CONSTANT Bug
ASSUME Bug \in {"none","bad-admission","late-copy","reordered","early-exit"}
VARIABLES phase, meanings, ownerLive, heap, buffer, copied, borrowed, expected
vars == <<phase,meanings,ownerLive,heap,buffer,copied,borrowed,expected>>
Init == /\ phase = "request" /\ meanings \in BOOLEAN /\ ownerLive = TRUE
        /\ heap = {1,2,3,4,5} /\ buffer = <<>> /\ copied = <<>>
        /\ borrowed = FALSE /\ expected = <<2,3,2>>
Roots == IF ownerLive THEN {1} ELSE {}
Reach == Roots \cup (IF ownerLive THEN {2,3} ELSE {})
Needed == IF phase \in {"body","copy"} THEN {1} \cup {copied[i]: i \in 1..Len(copied)} ELSE {}
Request == /\ phase = "request"
           /\ borrowed' = (meanings \/ Bug = "bad-admission")
           /\ phase' = IF meanings \/ Bug = "bad-admission" THEN "copy" ELSE "fallback"
           /\ buffer' = IF meanings THEN expected ELSE <<4,5,4>>
           /\ UNCHANGED <<meanings,ownerLive,heap,copied,expected>>
(* Canonical success has no callback before the caller copies. A later request
   may overwrite this single owner buffer; copied Oop identities remain intact. *)
Overwrite == /\ (phase = "body" \/ (phase = "copy" /\ Bug = "late-copy"))
             /\ buffer' = <<3,2,3>>
             /\ UNCHANGED <<phase,meanings,ownerLive,heap,copied,borrowed,expected>>
Copy == /\ phase = "copy" /\ phase' = "body"
        /\ copied' = IF Bug = "reordered" THEN <<buffer[2],buffer[1],buffer[3]>> ELSE buffer
        /\ UNCHANGED <<meanings,ownerLive,heap,buffer,borrowed,expected>>
Collect == /\ heap' = heap \cap Reach
           /\ UNCHANGED <<phase,meanings,ownerLive,buffer,copied,borrowed,expected>>
EarlyExit == /\ Bug = "early-exit" /\ phase = "body" /\ ownerLive' = FALSE
             /\ UNCHANGED <<phase,meanings,heap,buffer,copied,borrowed,expected>>
Finish == /\ phase \in {"body","fallback"} /\ phase' = "done" /\ ownerLive' = FALSE
          /\ copied' = <<>>
          /\ UNCHANGED <<meanings,heap,buffer,borrowed,expected>>
Next == Request \/ Overwrite \/ Copy \/ Collect \/ EarlyExit \/ Finish
Spec == Init /\ [][Next]_vars
NeededAlive == Needed \subseteq heap
SameOrderAndAliases == phase = "body" => copied = expected
BorrowOnlyCurrent == borrowed => meanings
=============================================================================
