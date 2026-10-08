------------------------- MODULE LocalReuse --------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none","unrooted-hint","eviction-drops-root","stale-scope","broken-edge","early-release","leak"}
VARIABLES phase, round, heap, tokens, hints, needed, receiverLive, edgesLive, acquired
vars == <<phase,round,heap,tokens,hints,needed,receiverLive,edgesLive,acquired>>
(* Receiver 1 captures 2. Fresh locals 3/4 exercise a one-slot hint cache. *)
Init == /\ phase = "body" /\ round = 0 /\ heap = {1,2}
        /\ tokens = {} /\ hints = {2} /\ needed = {1,2}
        /\ receiverLive = TRUE /\ edgesLive = TRUE /\ acquired = {}
Roots == tokens \cup (IF receiverLive THEN {1} ELSE {})
Reach == Roots \cup (IF receiverLive /\ edgesLive THEN {2} ELSE {})
Keep(value) == /\ phase = "body" /\ value \in {2,3,4}
               /\ heap' = heap \cup {value} /\ needed' = needed \cup {value}
               /\ tokens' = IF value \in hints \/ Bug = "unrooted-hint" THEN tokens
                             ELSE (IF Bug = "eviction-drops-root" THEN {} ELSE tokens) \cup {value}
               /\ hints' = {value} /\ acquired' = acquired \cup {value}
               /\ UNCHANGED <<phase,round,receiverLive,edgesLive>>
Collect == /\ heap' = heap \cap Reach
           /\ UNCHANGED <<phase,round,tokens,hints,needed,receiverLive,edgesLive,acquired>>
BreakEdge == /\ Bug = "broken-edge" /\ phase = "body" /\ edgesLive' = FALSE
             /\ UNCHANGED <<phase,round,heap,tokens,hints,needed,receiverLive,acquired>>
EarlyRelease == /\ Bug = "early-release" /\ phase = "body" /\ tokens # {} /\ tokens' = {}
                /\ UNCHANGED <<phase,round,heap,hints,needed,receiverLive,edgesLive,acquired>>
Finish == /\ phase = "body" /\ acquired = {2,3,4} /\ phase' = "done"
          /\ needed' = {} /\ receiverLive' = FALSE
          /\ tokens' = IF Bug = "leak" THEN tokens ELSE {}
          /\ hints' = IF Bug = "stale-scope" THEN hints ELSE {}
          /\ UNCHANGED <<round,heap,edgesLive,acquired>>
(* New activation/allocation: old pointer identities do not carry protection. *)
NextCall == /\ phase = "done" /\ round = 0 /\ phase' = "body" /\ round' = 1
            /\ heap' = {1,2} /\ needed' = {1,2} /\ receiverLive' = TRUE
            /\ edgesLive' = TRUE /\ acquired' = {}
            /\ UNCHANGED <<tokens,hints>>
Next == (\E value \in {2,3,4}: Keep(value)) \/ Collect \/ BreakEdge \/ EarlyRelease \/ Finish \/ NextCall
Spec == Init /\ [][Next]_vars
NeededAlive == needed \subseteq heap
NoLeakedProtection == phase = "done" => tokens = {}
=============================================================================
