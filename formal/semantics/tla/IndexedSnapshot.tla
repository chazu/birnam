------------------------ MODULE IndexedSnapshot ------------------------
EXTENDS Naturals, Sequences, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none", "bounds", "replay", "stale", "early-release"}
VARIABLES stage, mode, pos, history, expected, binding, pinned, alive, bad, failure
vars == <<stage,mode,pos,history,expected,binding,pinned,alive,bad,failure>>
(* Three delivery slots have deliberately non-index allocation identities.
   This models endpoint transport only; ordinary graph validation is retained. *)
Inventory == <<17,42,99>>
Init == /\ stage = "negotiate" /\ mode = "none" /\ pos = 1
        /\ history = <<>> /\ expected = <<>> /\ binding = 0
        /\ pinned = TRUE /\ alive = TRUE /\ bad \in BOOLEAN /\ failure = FALSE
Select(m) == /\ stage = "negotiate" /\ m \in {"scalar","indexed"}
             /\ mode' = m /\ stage' = "facts"
             /\ UNCHANGED <<pos,history,expected,binding,pinned,alive,bad,failure>>
Replace == /\ stage = "facts" /\ binding = 0 /\ binding' = 1
           /\ UNCHANGED <<stage,mode,pos,history,expected,pinned,alive,bad,failure>>
Emit == /\ stage = "facts" /\ pos <= 3
        /\ pos' = pos + 1
        /\ history' = Append(history, <<Inventory[pos], IF Bug = "stale" THEN 0 ELSE binding>>)
        /\ expected' = Append(expected, <<Inventory[pos], binding>>)
        /\ pinned' = IF Bug = "early-release" THEN FALSE ELSE pinned
        /\ UNCHANGED <<stage,mode,binding,alive,bad,failure>>
(* A withdrawn next callback fails; do not switch protocol after prior facts. *)
Withdraw == /\ stage = "facts" /\ pos > 1 /\ pos <= 3
            /\ failure' = TRUE /\ stage' = "cleanup"
            /\ history' = IF Bug = "replay" THEN history \o history ELSE history
            /\ UNCHANGED <<mode,pos,expected,binding,pinned,alive,bad>>
Complete == /\ stage = "facts" /\ pos = 4
            /\ stage' = IF bad /\ Bug # "bounds" THEN "cleanup" ELSE "report"
            /\ failure' = (bad /\ Bug # "bounds")
            /\ UNCHANGED <<mode,pos,history,expected,binding,pinned,alive,bad>>
Report == /\ stage = "report" /\ stage' = "cleanup"
          /\ UNCHANGED <<mode,pos,history,expected,binding,pinned,alive,bad,failure>>
Collect == /\ stage \in {"facts","report","cleanup"}
           /\ alive' = pinned
           /\ UNCHANGED <<stage,mode,pos,history,expected,binding,pinned,bad,failure>>
Cleanup == /\ stage = "cleanup" /\ stage' = "done" /\ pinned' = FALSE
           /\ UNCHANGED <<mode,pos,history,expected,binding,alive,bad,failure>>
Next == (\E m \in {"scalar","indexed"}: Select(m)) \/ Replace \/ Emit \/ Withdraw \/ Complete \/ Report \/ Collect \/ Cleanup
Spec == Init /\ [][Next]_vars
CurrentCallbacks == history = expected
AllValidated == stage = "report" => ~bad
InventoryAlive == stage \in {"facts","report","cleanup"} => alive
NoLeak == stage = "done" => ~pinned
==========================================================================
