------------------------- MODULE RelationalQuery -------------------------
EXTENDS Naturals, Sequences, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none","cached","shared","expired","unlatched","replay"}
Q == {1,2}
VARIABLES phase, facts, kept, alive, returned, failed, comparisons, meaning,
          edits, history, expected, outcome
vars == <<phase,facts,kept,alive,returned,failed,comparisons,meaning,edits,history,expected,outcome>>
Init == /\ phase = [q \in Q |-> "new"] /\ facts = [q \in Q |-> {}]
        /\ kept = {} /\ alive = {} /\ returned = {} /\ failed = [q \in Q |-> FALSE]
        /\ comparisons = [q \in Q |-> 0] /\ meaning = 0 /\ edits = 0
        /\ history = <<>> /\ expected = <<>> /\ outcome = [q \in Q |-> "none"]
Start(q) == /\ phase[q] = "new" /\ (q = 1 \/ phase[1] = "evaluate")
            /\ phase' = [phase EXCEPT ![q] = "collect"]
            /\ kept' = IF Bug = "shared" THEN {q} ELSE kept \cup {q}
            /\ alive' = alive \cup {q}
            /\ UNCHANGED <<facts,returned,failed,comparisons,meaning,edits,history,expected,outcome>>
Emit(q) == /\ phase[q] = "collect" /\ facts[q] = {}
           /\ facts' = [facts EXCEPT ![q] = {q}]
           /\ UNCHANGED <<phase,kept,alive,returned,failed,comparisons,meaning,edits,history,expected,outcome>>
Seal(q) == /\ phase[q] = "collect" /\ facts[q] = {q}
           /\ phase' = [phase EXCEPT ![q] = "evaluate"]
           /\ UNCHANGED <<facts,kept,alive,returned,failed,comparisons,meaning,edits,history,expected,outcome>>
Compare(q) == /\ phase[q] = "evaluate" /\ ~failed[q] /\ comparisons[q] < 2
              /\ comparisons' = [comparisons EXCEPT ![q] = @ + 1]
              /\ history' = Append(history, <<q, IF Bug = "cached" THEN 0 ELSE meaning>>)
              /\ expected' = Append(expected, <<q,meaning>>)
              /\ UNCHANGED <<phase,facts,kept,alive,returned,failed,meaning,edits,outcome>>
Replace == /\ edits < 2 /\ meaning' = 1 - meaning /\ edits' = edits + 1
           /\ UNCHANGED <<phase,facts,kept,alive,returned,failed,comparisons,history,expected,outcome>>
Fail(q) == /\ phase[q] \in {"collect","evaluate"} /\ ~failed[q]
           /\ failed' = [failed EXCEPT ![q] = TRUE]
           /\ UNCHANGED <<phase,facts,kept,alive,returned,comparisons,meaning,edits,history,expected,outcome>>
Finish(q) == /\ phase[q] = "evaluate" /\ (comparisons[q] = 2 \/ failed[q])
             /\ phase' = [phase EXCEPT ![q] = "done"] /\ facts' = [facts EXCEPT ![q] = {}]
             /\ kept' = kept \ {q}
             /\ outcome' = [outcome EXCEPT ![q] = IF failed[q] /\ Bug # "unlatched" THEN "failure" ELSE "success"]
             /\ returned' = IF failed[q] THEN returned ELSE returned \cup {q}
             /\ history' = IF Bug = "replay" THEN history \o history ELSE history
             /\ UNCHANGED <<alive,failed,comparisons,meaning,edits,expected>>
Late(q) == /\ phase[q] = "done" /\ facts[q] = {}
           /\ facts' = IF Bug = "expired" THEN [facts EXCEPT ![q] = {q}] ELSE facts
           /\ UNCHANGED <<phase,kept,alive,returned,failed,comparisons,meaning,edits,history,expected,outcome>>
GC == /\ alive' = alive \cap (kept \cup returned)
      /\ UNCHANGED <<phase,facts,kept,returned,failed,comparisons,meaning,edits,history,expected,outcome>>
Next == (\E q \in Q : Start(q) \/ Emit(q) \/ Seal(q) \/ Compare(q) \/ Fail(q) \/ Finish(q) \/ Late(q)) \/ Replace \/ GC
Spec == Init /\ [][Next]_vars
CurrentComparison == history = expected
SnapshotsAlive == \A q \in Q : phase[q] \in {"collect","evaluate"} => q \in alive
ClosedCollectors == \A q \in Q : phase[q] = "done" => facts[q] = {}
FailuresLatched == \A q \in Q : outcome[q] = "success" => ~failed[q]
NoLeakedScope == \A q \in Q : phase[q] = "done" => q \notin kept
==========================================================================
