----------------------------- MODULE SlotHints -----------------------------
EXTENDS Naturals, Sequences
CONSTANT Bug
ASSUME Bug \in {"none","key","value","parent"}
VARIABLES rows, parentValue, position, savedValue, savedParent, stage, answer, expected
vars == <<rows,parentValue,position,savedValue,savedParent,stage,answer,expected>>
(* Selector 1 is requested. Zero is an opaque/nonmethod shadow, not absence. *)
Local(r) == IF Len(r) >= 1 /\ r[1][1] = 1 THEN <<TRUE,r[1][2]>>
            ELSE IF Len(r) >= 2 /\ r[2][1] = 1 THEN <<TRUE,r[2][2]>> ELSE <<FALSE,0>>
Lookup(r,p) == IF Local(r)[1] THEN Local(r)[2] ELSE p
Init == /\ rows = <<<<1,10>>,<<2,20>>>> /\ parentValue = 30
        /\ position \in {1,2,3} /\ savedValue = 10 /\ savedParent = 30
        /\ stage = "mutate" /\ answer = 0 /\ expected = 0
Mutate == /\ stage = "mutate" /\ stage' = "read"
          /\ rows' \in {<<>>,<<<<1,99>>>>,<<<<2,20>>,<<1,99>>>>,<<<<1,0>>,<<2,20>>>>,<<<<2,20>>>>}
          /\ parentValue' \in {30,77}
          /\ UNCHANGED <<position,savedValue,savedParent,answer,expected>>
Read == /\ stage = "read" /\ stage' = "done"
        /\ expected' = Lookup(rows,parentValue)
        /\ answer' = IF position <= Len(rows) /\ (Bug = "key" \/ rows[position][1] = 1)
                      THEN IF Bug = "value" THEN savedValue ELSE rows[position][2]
                      ELSE Lookup(rows,IF Bug = "parent" THEN savedParent ELSE parentValue)
        /\ UNCHANGED <<rows,parentValue,position,savedValue,savedParent>>
Next == Mutate \/ Read
Spec == Init /\ [][Next]_vars
CurrentLookup == stage = "done" => answer = expected
=============================================================================
