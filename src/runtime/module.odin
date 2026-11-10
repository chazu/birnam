package services

import "../abi"

// Runtime services: every Birnam program's Integer/String/Symbol/Vector/Bytes
// behaviour, Cell, Environment, Block, interpreted Methods, Reflect, System,
// and the Object/nil/Boolean/Error protocol. No parser or syntax lives here.
State :: struct {
	object, integer, text, block, reflect, system, symbol_factory, cell, environment_factory: Oop,
	integer_vt, text_vt, block_vt, environment_vt, interpreted_method_vt, cell_vt: Oop,
}
state: State

symbol_from_bytes :: proc "c" (m, r, a: Oop) -> Oop { return birnam_intern(arg(a, 0)) }

// The prototype Block's code. The runtime owns no syntax, so this private
// object only answers evalIn:, and the prototype Block's value is nil.
empty_code_eval :: proc "c" (m, r, a: Oop) -> Oop { return abi.known.nil_value }

@(export)
birnam_module_init_v1 :: proc "c" (method, world, args: Oop) -> Oop {
	if state.object != nil { return abi.known.world }
	abi.attach(world)
	GC_add_roots(rawptr(&state), rawptr(uintptr(&state)+size_of(State)))
	state.object = send(world, "at:", symbol("Object"))
	state.integer_vt = new_behavior()
	state.text_vt = new_behavior()
	state.block_vt = new_behavior()
	state.environment_vt = new_behavior()
	state.interpreted_method_vt = send(birnam_method_behavior(), "delegated")
	state.cell_vt = new_behavior()
	empty_code_vt := new_behavior()

	install(empty_code_vt, "evalIn:", empty_code_eval, 1)
	install(state.environment_vt, "lookup:", environment_lookup, 1)
	install(state.environment_vt, "define:as:", environment_define, 2)
	install(state.environment_vt, "namespace", environment_namespace, 0)
	install(state.environment_vt, "parent", environment_parent, 0)
	install(state.environment_vt, "names", environment_names, 0)
	install(state.environment_vt, "forNamespace:", environment_for_namespace, 1)
	state.environment_factory = new_environment(world, abi.known.nil_value, true)

	state.integer = new_integer(0)
	install(state.integer_vt, "fromBytes:", integer_from_bytes, 1)
	install(state.integer_vt, "asInteger", identity_entry, 0)
	install(state.integer_vt, "printString", integer_print, 0)
	install(state.integer_vt, "+", integer_add, 1)
	install(state.integer_vt, "-", integer_subtract, 1)
	install(state.integer_vt, "*", integer_multiply, 1)
	install(state.integer_vt, "/", integer_divide, 1)
	install(state.integer_vt, "%", integer_remainder, 1)
	install(state.integer_vt, "=", integer_equal, 1)
	install(state.integer_vt, "<", integer_less, 1)
	install(state.integer_vt, ">", integer_greater, 1)
	install(state.integer_vt, "<=", integer_le, 1)
	install(state.integer_vt, ">=", integer_ge, 1)
	state.text = new_string("")
	install(state.text_vt, "fromBytes:", string_from_bytes, 1)
	install(state.text_vt, "asBytes", string_bytes, 0)
	install(state.text_vt, "printString", identity_entry, 0)
	install(state.text_vt, "intern", string_intern, 0)
	install(state.text_vt, ",", string_concat, 1)
	install(state.text_vt, "=", string_equal, 1)
	text_behaviors := [2]Oop{state.text_vt, birnam_bytes_behavior()}
	for behavior in text_behaviors {
		install(behavior, "byteSize", text_byte_size, 0)
		install(behavior, "utf8ErrorOffset", text_utf8_error_offset, 0)
		install(behavior, "byteAt:", text_byte_at, 1)
		install(behavior, "bytesFrom:to:", text_slice, 2)
		install(behavior, "commonPrefixLengthWith:at:", text_prefix_length, 2)
	}
	install(birnam_bytes_behavior(), "asString", bytes_string, 0)
	install(birnam_bytes_behavior(), "printString", bytes_string, 0)
	install(header(symbol("symbol" )).behavior, "printString", symbol_print, 0)
	state.symbol_factory = send(state.object, "clone")
	local_method(state.symbol_factory, "fromBytes:", symbol_from_bytes, 1)
	install(birnam_vector_behavior(), "fromElements:", vector_from_elements, 1)
	install(birnam_vector_behavior(), "size", vector_size, 0)
	install(birnam_vector_behavior(), "at:", vector_at, 1)
	install(birnam_vector_behavior(), "at:put:", vector_put, 2)
	install(birnam_vector_behavior(), "append:", vector_append, 1)
	install(birnam_vector_behavior(), "concat:", vector_concat, 1)
	state.cell = allocate(state.cell_vt, size_of(Oop))
	(cast(^Oop)state.cell)^ = abi.known.nil_value
	install(state.cell_vt, "with:", cell_with, 1)
	install(state.cell_vt, "value", cell_value, 0)
	install(state.cell_vt, "value:", cell_set, 1)

	state.block = allocate(state.block_vt, size_of(Function))
	empty_code := allocate(empty_code_vt, 0)
	(cast(^Function)state.block)^ = Function{arguments(), empty_code, state.environment_factory}
	install(state.block_vt, "code:environment:parameters:", block_create, 3)
	install(state.block_vt, "value", block_value, 0)
	install(state.block_vt, "value:", block_value, 1)
	install(state.block_vt, "value:value:", block_value, 2)
	install(state.block_vt, "value:value:value:", block_value, 3)
	install(state.block_vt, "value:value:value:value:", block_value, 4)
	install(state.block_vt, "valueWithArguments:", block_value_arguments, 1)
	install(state.block_vt, "code", block_code, 0)
	install(state.block_vt, "parameters", block_parameters, 0)
	install(state.block_vt, "environment", block_environment, 0)
	install(state.block_vt, "whileTrue:", block_while_true, 1)
	install(state.interpreted_method_vt, "code", block_code, 0)
	install(state.interpreted_method_vt, "parameters", block_parameters, 0)
	install(state.interpreted_method_vt, "environment", block_environment, 0)
	state.reflect = send(state.object, "clone")
	local_method(state.reflect, "methodFrom:", reflect_method_from, 1)
	local_method(state.reflect, "methodFor:on:", reflect_method_for, 2)
	local_method(state.reflect, "send:to:with:", reflect_send, 3)
	local_method(state.reflect, "defineMethod:on:as:", reflect_define, 3)
	install(birnam_method_behavior(), "valueOn:with:", method_value_on, 2)

	install(birnam_object_behavior(), "printString", object_print, 0)
	install(birnam_object_behavior(), "asInteger", conversion_error, 0)
	install(birnam_object_behavior(), "asBytes", conversion_error, 0)
	install(birnam_object_behavior(), "isNil", answer_false, 0)
	install(birnam_object_behavior(), "ifError:", identity_entry, 1)
	install(birnam_object_behavior(), "ifNotError:", success_apply, 1)
	local_method(abi.known.nil_value, "isNil", answer_true, 0)
	local_method(abi.known.nil_value, "printString", nil_print, 0)
	booleans := [2]Oop{abi.known.yes, abi.known.no}
	for value in booleans {
		local_method(value, "ifTrue:ifFalse:", if_true_false, 2)
		local_method(value, "ifTrue:", if_true, 1)
		local_method(value, "ifFalse:", if_false, 1)
		local_method(value, "not", bool_not, 0)
		local_method(value, "and:", bool_and, 1)
		local_method(value, "or:", bool_or, 1)
		local_method(value, "printString", bool_print, 0)
	}
	error_vt := send(abi.known.error, "behavior")
	install(error_vt, "ifError:", error_recover, 1)
	install(error_vt, "ifNotError:", identity_entry, 1)
	install(error_vt, "printString", error_print, 0)
	state.system = send(state.object, "clone")
	local_method(state.system, "print:", system_print, 1)
	local_method(state.system, "println:", system_println, 1)
	publish("Integer", state.integer)
	publish("String", state.text)
	publish("Symbol", state.symbol_factory)
	publish("Block", state.block)
	publish("Environment", state.environment_factory)
	publish("Reflect", state.reflect)
	publish("System", state.system)
	publish("Cell", state.cell)
	return world
}
