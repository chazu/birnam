------------------------ MODULE ResultHandoff ------------------------
EXTENDS Naturals, Sequences, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none","missing-scope","early-exit","wrong-order"}
VARIABLES heap, tokens, scope, caller, stage, released
vars == <<heap,tokens,scope,caller,stage,released>>
Init == /\ heap = {1,2,3} /\ tokens = <<1,2,3>> /\ scope = FALSE
        /\ caller = FALSE /\ stage = "enter" /\ released = <<>>
Roots == {tokens[i]:i \in 1..Len(tokens)} \cup (IF scope \/ caller THEN {3} ELSE {})
Collect == /\ heap' = heap \cap Roots
           /\ UNCHANGED <<tokens,scope,caller,stage,released>>
Enter == /\ stage = "enter" /\ stage' = "cleanup"
         /\ scope' = (Bug # "missing-scope")
         /\ UNCHANGED <<heap,tokens,caller,released>>
Release == /\ stage = "cleanup" /\ Len(tokens) > 0
           /\ LET chosen == IF Bug = "wrong-order" THEN Head(tokens) ELSE tokens[Len(tokens)]
                  remaining == IF Bug = "wrong-order" THEN Tail(tokens) ELSE SubSeq(tokens,1,Len(tokens)-1)
              IN /\ tokens' = remaining /\ released' = Append(released,chosen)
           /\ scope' = IF Bug = "early-exit" THEN FALSE ELSE scope
           /\ UNCHANGED <<heap,caller,stage>>
(* Each current release callback may collect. Return/caller transfer has no
   intervening callback, so ownership overlap ends only at this atomic step. *)
Transfer == /\ stage = "cleanup" /\ Len(tokens) = 0
            /\ stage' = "done" /\ caller' = TRUE /\ scope' = FALSE
            /\ UNCHANGED <<heap,tokens,released>>
Next == Collect \/ Enter \/ Release \/ Transfer
Spec == Init /\ [][Next]_vars
ResultAlive == 3 \in heap
ReleaseOrder == released = SubSeq(<<3,2,1>>,1,Len(released))
NoLeakedScope == stage = "done" => ~scope
======================================================================
