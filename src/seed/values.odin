package seed

import "base:runtime"
import "core:fmt"
import "core:unicode/utf8"

utf8_error_offset :: proc "contextless" (text: string) -> int {
	i := 0
	for i < len(text) {
		r, width := utf8.decode_rune(text[i:])
		if r == utf8.RUNE_ERROR && width == 1 { return i }
		i += width
	}
	return -1
}
new_string :: proc "contextless" (text: string) -> Oop {
	result := allocate(state.text_vt, uintptr(len(text)))
	copy((cast([^]u8)result)[:len(text)], transmute([]u8)text)
	return result
}
new_integer :: proc "contextless" (value: i64) -> Oop {
	result := allocate(state.integer_vt, size_of(i64))
	(cast(^i64)result)^ = value
	return result
}
boolean :: proc "contextless" (value: bool) -> Oop { return state.yes if value else state.no }
parse_decimal :: proc "contextless" (text: string) -> (i64, bool) {
	if len(text) == 0 { return 0, false }
	i, negative := 0, false
	if text[0] == '-' { negative = true; i = 1 }
	if i == len(text) { return 0, false }
	limit := i128(max(i64)) + (1 if negative else 0)
	n: i128
	for i < len(text) {
		ch := text[i]
		if ch < '0' || ch > '9' { return 0, false }
		n = n*10 + i128(ch-'0')
		if n > limit { return 0, false }
		i += 1
	}
	return i64(-n if negative else n), true
}
integer_operand :: proc "contextless" (value: Oop) -> (i64, Oop) {
	converted := send(value, "asInteger")
	if is_error(converted) { return 0, converted }
	if header(converted).payload_bytes != size_of(i64) || !has_behavior(converted, state.integer_vt) {
		return 0, fail("typeMismatch", "asInteger must return a native Integer")
	}
	return (cast(^i64)converted)^, nil
}
integer_from_bytes :: proc "c" (m, r, a: Oop) -> Oop {
	input := as_bytes(arg(a, 0))
	if is_error(input) { return input }
	value, ok := parse_decimal(native_text(input))
	if !ok { return fail("integerRange", "expected a signed 64-bit decimal Integer") }
	return new_integer(value)
}
identity_entry :: proc "c" (m, r, a: Oop) -> Oop { return r }
conversion_error :: proc "c" (m, r, a: Oop) -> Oop { return fail("typeMismatch", "object does not support the requested conversion") }
object_print :: proc "c" (m, r, a: Oop) -> Oop { return new_string("Object") }
integer_print :: proc "c" (m, r, a: Oop) -> Oop {
	context = runtime.default_context()
	buffer: [32]u8
	return new_string(fmt.bprintf(buffer[:], "%d", (cast(^i64)r)^))
}

// Operation tags are private choices inside Integer's selected native
// implementations. They are not value/AST tags in the evaluator.
Arithmetic :: enum { Add, Subtract, Multiply, Divide, Remainder }
integer_arithmetic :: proc "contextless" (r, operand: Oop, op: Arithmetic) -> Oop {
	x, problem := integer_operand(r)
	if problem != nil { return problem }
	y, other_problem := integer_operand(operand)
	if other_problem != nil { return other_problem }
	wide: i128
	switch op {
	case .Add: wide = i128(x)+i128(y)
	case .Subtract: wide = i128(x)-i128(y)
	case .Multiply: wide = i128(x)*i128(y)
	case .Divide:
		if y == 0 { return fail("zeroDivide", "division by zero") }
		wide = i128(x)/i128(y)
	case .Remainder:
		if y == 0 { return fail("zeroDivide", "remainder by zero") }
		wide = i128(x)%i128(y)
	}
	if wide < i128(min(i64)) || wide > i128(max(i64)) { return fail("integerOverflow", "Integer result is outside signed 64-bit range") }
	return new_integer(i64(wide))
}
integer_add :: proc "c" (m, r, a: Oop) -> Oop { return integer_arithmetic(r, arg(a, 0), .Add) }
integer_subtract :: proc "c" (m, r, a: Oop) -> Oop { return integer_arithmetic(r, arg(a, 0), .Subtract) }
integer_multiply :: proc "c" (m, r, a: Oop) -> Oop { return integer_arithmetic(r, arg(a, 0), .Multiply) }
integer_divide :: proc "c" (m, r, a: Oop) -> Oop { return integer_arithmetic(r, arg(a, 0), .Divide) }
integer_remainder :: proc "c" (m, r, a: Oop) -> Oop { return integer_arithmetic(r, arg(a, 0), .Remainder) }
Comparison :: enum { Equal, Less, Greater, LessEqual, GreaterEqual }
integer_compare :: proc "contextless" (r, operand: Oop, op: Comparison) -> Oop {
	x, error := integer_operand(r)
	if error != nil { return error }
	y, other_error := integer_operand(operand)
	if other_error != nil { return other_error }
	switch op {
	case .Equal: return boolean(x == y)
	case .Less: return boolean(x < y)
	case .Greater: return boolean(x > y)
	case .LessEqual: return boolean(x <= y)
	case .GreaterEqual: return boolean(x >= y)
	}
	unreachable()
}
integer_equal :: proc "c" (m, r, a: Oop) -> Oop { return integer_compare(r, arg(a, 0), .Equal) }
integer_less :: proc "c" (m, r, a: Oop) -> Oop { return integer_compare(r, arg(a, 0), .Less) }
integer_greater :: proc "c" (m, r, a: Oop) -> Oop { return integer_compare(r, arg(a, 0), .Greater) }
integer_le :: proc "c" (m, r, a: Oop) -> Oop { return integer_compare(r, arg(a, 0), .LessEqual) }
integer_ge :: proc "c" (m, r, a: Oop) -> Oop { return integer_compare(r, arg(a, 0), .GreaterEqual) }

string_from_bytes :: proc "c" (m, r, a: Oop) -> Oop {
	value := as_bytes(arg(a, 0))
	if is_error(value) { return value }
	text := native_text(value)
	if utf8_error_offset(text) >= 0 { return fail("invalidUtf8", "Bytes are not valid UTF-8") }
	return new_string(text)
}
string_bytes :: proc "c" (m, r, a: Oop) -> Oop { return new_bytes(native_text(r)) }
text_byte_size :: proc "c" (m, r, a: Oop) -> Oop { return new_integer(i64(header(r).payload_bytes)) }
text_byte_at :: proc "c" (m, r, a: Oop) -> Oop {
	index, error := integer_operand(arg(a, 0))
	if error != nil { return error }
	if index < 0 || uintptr(index) >= header(r).payload_bytes { return fail("indexOutOfBounds", "byte index is outside the buffer") }
	return new_integer(i64((cast([^]u8)r)[index]))
}
text_slice :: proc "c" (m, r, a: Oop) -> Oop {
	start, error := integer_operand(arg(a, 0))
	if error != nil { return error }
	end, other := integer_operand(arg(a, 1))
	if other != nil { return other }
	if start < 0 || end < start || uintptr(end) > header(r).payload_bytes { return fail("indexOutOfBounds", "invalid half-open byte span") }
	return new_bytes(native_text(r)[int(start):int(end)])
}
bytes_string :: proc "c" (m, r, a: Oop) -> Oop { return send(state.text, "fromBytes:", r) }
string_intern :: proc "c" (m, r, a: Oop) -> Oop { return birnam_intern(send(r, "asBytes")) }
string_concat :: proc "c" (m, r, a: Oop) -> Oop {
	rendered := send(arg(a, 0), "printString")
	if is_error(rendered) { return rendered }
	if !has_behavior(rendered, state.text_vt) { return fail("typeMismatch", "printString must return String") }
	left, right := native_text(r), native_text(rendered)
	result := allocate(state.text_vt, uintptr(len(left)+len(right)))
	data := (cast([^]u8)result)[:len(left)+len(right)]
	copy(data, transmute([]u8)left)
	copy(data[len(left):], transmute([]u8)right)
	return result
}
string_equal :: proc "c" (m, r, a: Oop) -> Oop {
	other := as_bytes(arg(a, 0))
	if is_error(other) { return other }
	return boolean(native_text(r) == native_text(other))
}
symbol_print :: proc "c" (m, r, a: Oop) -> Oop {
	text := new_string("#")
	return send(text, ",", new_string(native_text(r)))
}

vector_size :: proc "c" (m, r, a: Oop) -> Oop { return new_integer(i64(count(r))) }
vector_from_elements :: proc "c" (m, r, a: Oop) -> Oop {
	input := arg(a, 0)
	if !valid_vector(input) { return fail("typeMismatch", "Vector elements must be a Vector") }
	result := new_arguments(count(input))
	for i in 0..<count(input) { items(result)[i] = arg(input, i) }
	return result
}
vector_at :: proc "c" (m, r, a: Oop) -> Oop {
	index, error := integer_operand(arg(a, 0))
	if error != nil { return error }
	if index < 0 || index >= i64(count(r)) { return fail("indexOutOfBounds", "Vector index is outside its extent") }
	return arg(r, int(index))
}
vector_put :: proc "c" (m, r, a: Oop) -> Oop {
	index, error := integer_operand(arg(a, 0))
	if error != nil { return error }
	if index < 0 || index >= i64(count(r)) { return fail("indexOutOfBounds", "Vector index is outside its extent") }
	result := new_arguments(count(r))
	for i in 0..<count(r) { items(result)[i] = arg(r, i) }
	items(result)[int(index)] = arg(a, 1)
	return result
}

if_true_false :: proc "c" (m, r, a: Oop) -> Oop { return send(arg(a, 0 if r == state.yes else 1), "value") }
if_true :: proc "c" (m, r, a: Oop) -> Oop { return send(arg(a, 0), "value") if r == state.yes else state.nil_value }
if_false :: proc "c" (m, r, a: Oop) -> Oop { return send(arg(a, 0), "value") if r == state.no else state.nil_value }
bool_not :: proc "c" (m, r, a: Oop) -> Oop { return boolean(r == state.no) }
bool_print :: proc "c" (m, r, a: Oop) -> Oop { return new_string("true" if r == state.yes else "false") }
nil_print :: proc "c" (m, r, a: Oop) -> Oop { return new_string("nil") }
answer_true :: proc "c" (m, r, a: Oop) -> Oop { return state.yes }
answer_false :: proc "c" (m, r, a: Oop) -> Oop { return state.no }
error_recover :: proc "c" (m, r, a: Oop) -> Oop { return send(arg(a, 0), "value:", r) }
success_apply :: proc "c" (m, r, a: Oop) -> Oop { return send(arg(a, 0), "value:", r) }
error_print :: proc "c" (m, r, a: Oop) -> Oop { return send(state.text, "fromBytes:", send(r, "description")) }

system_print :: proc "c" (m, r, a: Oop) -> Oop {
	context = runtime.default_context()
	text := send(arg(a, 0), "printString")
	if is_error(text) { return text }
	if !has_behavior(text, state.text_vt) { return fail("typeMismatch", "printString must return String") }
	fmt.print(native_text(text))
	return r
}
system_println :: proc "c" (m, r, a: Oop) -> Oop {
	context = runtime.default_context()
	result := system_print(m, r, a)
	if is_error(result) { return result }
	fmt.println()
	return r
}
