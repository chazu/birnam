-------------------------- MODULE CollectionSearch --------------------------
EXTENDS Naturals, Sequences
CONSTANT Bug
ASSUME Bug \in {"none", "replay", "stale", "root", "nil"}
VARIABLES phase, index, rows, initialRows, element, expectedElement, calls,
          answer, edited, parked, alive, version, enteredVersion, expectedVersion
vars == <<phase,index,rows,initialRows,element,expectedElement,calls,answer,
          edited,parked,alive,version,enteredVersion,expectedVersion>>
(* 0 denotes nil, 9 a possible changed value. An entered predicate keeps its
   generation; the next call sees the current version. No native frame is added. *)
Init == /\ rows \in {<<0,2,3>>, <<1,2,3>>} /\ initialRows = rows
        /\ phase = "read" /\ index = 1 /\ element = 0 /\ expectedElement = 0
        /\ calls = <<>> /\ answer = 0 /\ edited = FALSE /\ parked = FALSE
        /\ alive = TRUE /\ version = 1 /\ enteredVersion = 1 /\ expectedVersion = 1
Read == /\ phase = "read" /\ index <= Len(rows)
        /\ element' = IF Bug = "stale" THEN initialRows[index] ELSE rows[index]
        /\ expectedElement' = rows[index]
        /\ enteredVersion' = version /\ expectedVersion' = version
        /\ calls' = Append(calls,index) /\ phase' = "predicate" /\ parked' = FALSE
        /\ alive' = TRUE
        /\ UNCHANGED <<index,rows,initialRows,answer,edited,version>>
End == /\ phase = "read" /\ index > Len(rows) /\ phase' = "done"
       /\ UNCHANGED <<index,rows,initialRows,element,expectedElement,calls,answer,
                       edited,parked,alive,version,enteredVersion,expectedVersion>>
Park == /\ phase = "predicate" /\ ~parked /\ phase' = "parked" /\ parked' = TRUE
        /\ UNCHANGED <<index,rows,initialRows,element,expectedElement,calls,answer,
                        edited,alive,version,enteredVersion,expectedVersion>>
Edit == /\ phase = "parked" /\ ~edited /\ edited' = TRUE
        /\ rows' = <<9,9,9>> /\ version' = 2
        /\ UNCHANGED <<phase,index,initialRows,element,expectedElement,calls,answer,
                        parked,alive,enteredVersion,expectedVersion>>
GC == /\ phase = "parked" /\ alive
      /\ alive' = (Bug # "root")
      /\ UNCHANGED <<phase,index,rows,initialRows,element,expectedElement,calls,answer,
                      edited,parked,version,enteredVersion,expectedVersion>>
Resume == /\ phase = "parked" /\ phase' = "predicate"
          /\ calls' = IF Bug = "replay" THEN Append(calls,index) ELSE calls
          /\ UNCHANGED <<index,rows,initialRows,element,expectedElement,answer,
                          edited,parked,alive,version,enteredVersion,expectedVersion>>
Return == /\ phase = "predicate"
          /\ LET hit == (enteredVersion = 2 \/ element # 1)
                 stop == hit /\ (element # 0 \/ Bug = "nil")
             IN /\ phase' = IF stop THEN "done" ELSE "read"
                /\ answer' = IF stop THEN element ELSE 0
                /\ index' = IF stop THEN index ELSE index + 1
          /\ UNCHANGED <<rows,initialRows,element,expectedElement,calls,edited,
                          parked,alive,version,enteredVersion,expectedVersion>>
Cancel == /\ phase = "parked" /\ phase' = "cancelled"
          /\ UNCHANGED <<index,rows,initialRows,element,expectedElement,calls,answer,
                          edited,parked,alive,version,enteredVersion,expectedVersion>>
Next == Read \/ End \/ Park \/ Edit \/ GC \/ Resume \/ Return \/ Cancel
Spec == Init /\ [][Next]_vars
SearchContract ==
  /\ \A i \in 1..Len(calls): calls[i] = i
  /\ phase \in {"predicate","parked"} =>
       (element = expectedElement /\ enteredVersion = expectedVersion /\ alive)
  /\ (phase = "done" /\ answer = 0) => index > Len(rows)
=============================================================================
