------------------------ MODULE CaptureReads --------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none","fresh-unrooted","bad-grant","early-exit","mutated-edge"}
VARIABLES phase, index, readerDefault, grantDefault, borrow, ownerLive, edgesLive,
          protected, results, heap
vars == <<phase,index,readerDefault,grantDefault,borrow,ownerLive,edgesLive,
          protected,results,heap>>
(* Block 1 captures 2/3; a replacement allocates fresh results 4/5. *)
Init == /\ phase = "grant" /\ index = 0 /\ readerDefault = TRUE
        /\ grantDefault = TRUE /\ borrow = FALSE /\ ownerLive = TRUE
        /\ edgesLive = TRUE /\ protected = {} /\ results = {} /\ heap = {1,2,3}
Roots == protected \cup (IF ownerLive THEN {1} ELSE {})
Reach == Roots \cup (IF ownerLive /\ edgesLive THEN {2,3} ELSE {})
Needed == IF phase = "done" THEN {} ELSE {1} \cup results
Collect == /\ heap' = heap \cap Reach
           /\ UNCHANGED <<phase,index,readerDefault,grantDefault,borrow,ownerLive,
                          edgesLive,protected,results>>
(* The default reader has no callback. A wrapper grant or replacement reader
   must decline borrowing before its callback can change another binding. *)
Replace == /\ phase = "grant" \/ (phase = "read" /\ ~borrow) \/ phase = "body"
           /\ readerDefault' = FALSE /\ grantDefault' = FALSE
           /\ UNCHANGED <<phase,index,borrow,ownerLive,edgesLive,protected,results,heap>>
Grant == /\ phase = "grant" /\ phase' = "read"
         /\ borrow' = (IF Bug = "bad-grant" THEN TRUE ELSE readerDefault /\ grantDefault)
         /\ UNCHANGED <<index,readerDefault,grantDefault,ownerLive,edgesLive,protected,results,heap>>
(* Ordinary root acquisition protects a fresh return before another callback;
   that existing ABI/root-operation lifetime obligation is a model premise. *)
Read == /\ phase = "read" /\ index < 2
        /\ LET value == IF readerDefault THEN index + 2 ELSE index + 4
           IN /\ results' = results \cup {value}
              /\ heap' = heap \cup {value}
              /\ protected' = IF ~borrow /\ Bug # "fresh-unrooted"
                              THEN protected \cup {value} ELSE protected
        /\ index' = index + 1
        /\ phase' = IF index = 1 THEN "body" ELSE "read"
        /\ UNCHANGED <<readerDefault,grantDefault,borrow,ownerLive,edgesLive>>
EarlyExit == /\ Bug = "early-exit" /\ phase = "body" /\ ownerLive' = FALSE
             /\ UNCHANGED <<phase,index,readerDefault,grantDefault,borrow,edgesLive,protected,results,heap>>
BreakEdge == /\ Bug = "mutated-edge" /\ phase = "body" /\ edgesLive' = FALSE
             /\ UNCHANGED <<phase,index,readerDefault,grantDefault,borrow,ownerLive,protected,results,heap>>
Finish == /\ phase = "body" /\ phase' = "done" /\ ownerLive' = FALSE
          /\ protected' = {} /\ results' = {}
          /\ UNCHANGED <<index,readerDefault,grantDefault,borrow,edgesLive,heap>>
Next == Collect \/ Replace \/ Grant \/ Read \/ EarlyExit \/ BreakEdge \/ Finish
Spec == Init /\ [][Next]_vars
NeededAlive == Needed \subseteq heap
NoLeakedProtection == phase = "done" => ~ownerLive /\ protected = {}
=============================================================================
