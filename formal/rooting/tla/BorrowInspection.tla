------------------------ MODULE BorrowInspection -------------------------
EXTENDS Naturals
CONSTANT Bug
ASSUME Bug \in {"none", "callback-before-parse"}
VARIABLES bytes, copied, canonicalShape, borrowedShape, stage
vars == <<bytes,copied,canonicalShape,borrowedShape,stage>>
Init == /\ bytes \in {0,1} /\ copied = 0 /\ canonicalShape = 0 /\ borrowedShape = 0
        /\ stage = "before-fetch"
(* Fetch is the completed current owner bytes message. A caller-owned scope
   protects its Bytes object. Foreign mutations are possible at every send,
   but only pure native code runs between Fetch and extraction of scalars. *)
Fetch == /\ stage = "before-fetch" /\ copied' = bytes /\ stage' = "borrowed"
         /\ UNCHANGED <<bytes,canonicalShape,borrowedShape>>
Call == /\ stage # "borrowed" \/ Bug = "callback-before-parse"
        /\ bytes' = 1-bytes
        /\ UNCHANGED <<copied,canonicalShape,borrowedShape,stage>>
Parse == /\ stage = "borrowed" /\ canonicalShape' = copied /\ borrowedShape' = bytes
         /\ stage' = "owned-scalars"
         /\ UNCHANGED <<bytes,copied>>
Next == Fetch \/ Call \/ Parse
Spec == Init /\ [][Next]_vars
SameInspection == stage = "owned-scalars" => canonicalShape = borrowedShape
=============================================================================
