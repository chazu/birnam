package services

import "../abi"

// General storage/control/buffer operations. Parsing policies live in Birnam.
cell_with :: proc "c" (m, r, a: Oop) -> Oop {
	result := allocate(state.cell_vt, size_of(Oop))
	(cast(^Oop)result)^ = arg(a, 0)
	return result
}
cell_value :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Oop)r)^ }
cell_set :: proc "c" (m, r, a: Oop) -> Oop { (cast(^Oop)r)^ = arg(a, 0); return r }

block_while_true :: proc "c" (m, r, a: Oop) -> Oop {
	for {
		condition := send(r, "value")
		if is_error(condition) { return condition }
		if condition == abi.known.no { return abi.known.nil_value }
		if condition != abi.known.yes { return fail("nonBooleanCondition", "whileTrue: requires a Boolean condition") }
		result := send(arg(a, 0), "value")
		if is_error(result) { return result }
	}
}
bool_and :: proc "c" (m, r, a: Oop) -> Oop { return send(arg(a, 0), "value") if r == abi.known.yes else abi.known.no }
bool_or :: proc "c" (m, r, a: Oop) -> Oop { return abi.known.yes if r == abi.known.yes else send(arg(a, 0), "value") }

vector_append :: proc "c" (m, r, a: Oop) -> Oop {
	result := new_arguments(count(r)+1)
	for i in 0..<count(r) { items(result)[i] = arg(r, i) }
	items(result)[count(r)] = arg(a, 0)
	return result
}
vector_concat :: proc "c" (m, r, a: Oop) -> Oop {
	other := arg(a, 0)
	if !valid_vector(other) { return fail("typeMismatch", "concat: requires a Vector") }
	result := new_arguments(count(r)+count(other))
	for i in 0..<count(r) { items(result)[i] = arg(r, i) }
	for i in 0..<count(other) { items(result)[count(r)+i] = arg(other, i) }
	return result
}

text_prefix_length :: proc "c" (m, r, a: Oop) -> Oop {
	other := as_bytes(arg(a, 0))
	if is_error(other) { return other }
	start, problem := integer_operand(arg(a, 1))
	if problem != nil { return problem }
	if start < 0 || uintptr(start) > header(r).payload_bytes { return fail("indexOutOfBounds", "prefix comparison offset is outside the buffer") }
	left, right := native_text(r)[int(start):], native_text(other)
	i := 0
	for i < len(left) && i < len(right) && left[i] == right[i] { i += 1 }
	return new_integer(i64(i))
}
