------------------------ MODULE CaptureReads --------------------------
EXTENDS Naturals, FiniteSets
CONSTANT Bug
ASSUME Bug \in {"none","fresh-unrooted","bad-grant","early-exit","mutated-edge","cached-grant"}
VARIABLES phase, index, readerDefault, grantDefault, borrow, ownerLive, edgesLive,
          protected, results, heap, invocation, fallback, factoryLive
vars == <<phase,index,readerDefault,grantDefault,borrow,ownerLive,edgesLive,
          protected,results,heap,invocation,fallback,factoryLive>>
(* Block 1 captures 2/3; a replacement allocates fresh results 4/5. *)
Init == /\ phase = "grant" /\ index = 0 /\ readerDefault = TRUE
        /\ grantDefault = TRUE /\ borrow = FALSE /\ ownerLive = TRUE
        /\ edgesLive = TRUE /\ protected = {} /\ results = {} /\ heap = {1,2,3,6}
        /\ invocation = 0 /\ fallback \in BOOLEAN /\ factoryLive = fallback
Roots == protected \cup (IF ownerLive THEN {1} ELSE {})
                   \cup (IF factoryLive THEN {6} ELSE {})
Reach == Roots \cup (IF ownerLive /\ edgesLive THEN {2,3} ELSE {})
Needed == IF phase = "done" THEN {} ELSE {1} \cup results
          \cup (IF phase = "grant" /\ fallback THEN {6} ELSE {})
Collect == /\ heap' = heap \cap Reach
           /\ UNCHANGED <<phase,index,readerDefault,grantDefault,borrow,ownerLive,
                          edgesLive,protected,results,invocation,fallback,factoryLive>>
(* The default reader has no callback. A wrapper grant or replacement reader
   must decline borrowing before its callback can change another binding. *)
Replace == /\ phase = "grant" \/ (phase = "read" /\ ~borrow) \/ phase = "body"
           /\ readerDefault' = FALSE /\ grantDefault' = FALSE
           /\ UNCHANGED <<phase,index,borrow,ownerLive,edgesLive,protected,results,heap,
                          invocation,fallback,factoryLive>>
Grant == /\ phase = "grant" /\ phase' = "read"
         /\ borrow' = (IF Bug = "bad-grant" \/ (Bug = "cached-grant" /\ invocation = 1)
                        THEN TRUE ELSE readerDefault /\ grantDefault)
         /\ factoryLive' = FALSE
         /\ UNCHANGED <<index,readerDefault,grantDefault,ownerLive,edgesLive,protected,results,heap,
                        invocation,fallback>>
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
        /\ UNCHANGED <<readerDefault,grantDefault,borrow,ownerLive,edgesLive,
                       invocation,fallback,factoryLive>>
EarlyExit == /\ Bug = "early-exit" /\ phase = "body" /\ ownerLive' = FALSE
             /\ UNCHANGED <<phase,index,readerDefault,grantDefault,borrow,edgesLive,protected,results,heap,
                            invocation,fallback,factoryLive>>
BreakEdge == /\ Bug = "mutated-edge" /\ phase = "body" /\ edgesLive' = FALSE
             /\ UNCHANGED <<phase,index,readerDefault,grantDefault,borrow,ownerLive,protected,results,heap,
                            invocation,fallback,factoryLive>>
Finish == /\ phase = "body" /\ phase' = "done" /\ ownerLive' = FALSE
          /\ protected' = {} /\ results' = {}
          /\ UNCHANGED <<index,readerDefault,grantDefault,borrow,edgesLive,heap,
                         invocation,fallback,factoryLive>>
(* A new call must re-admit the CURRENT query/reader. Receiver edges survive
   between calls through an external owner; the model roots them again on entry.
   A fallback-only factory is reacquired before its query, never needed by reads. *)
NextCall == /\ phase = "done" /\ invocation = 0 /\ invocation' = 1
            /\ phase' = "grant" /\ index' = 0 /\ ownerLive' = TRUE
            /\ edgesLive' = TRUE /\ factoryLive' = fallback
            /\ heap' = heap \cup {1,2,3,6}
            /\ readerDefault' = FALSE /\ grantDefault' = FALSE
            /\ UNCHANGED <<borrow,protected,results,fallback>>
Next == Collect \/ Replace \/ Grant \/ Read \/ EarlyExit \/ BreakEdge \/ Finish \/ NextCall
Spec == Init /\ [][Next]_vars
NeededAlive == Needed \subseteq heap
NoLeakedProtection == phase = "done" => ~ownerLive /\ protected = {}
FactoryOnlyDuringFallback == factoryLive => fallback /\ phase = "grant"
=============================================================================
