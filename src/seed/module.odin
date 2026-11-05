package seed

import "base:runtime"
import "core:os"

seed_parse :: proc "c" (m, r, a: Oop) -> Oop {
	context = runtime.default_context()
	return parse_source(arg(a, 0))
}
seed_eval :: proc "c" (m, r, a: Oop) -> Oop { return send(r, "eval:in:", arg(a, 0), state.world) }
seed_eval_in :: proc "c" (m, r, a: Oop) -> Oop {
	program := send(r, "parse:", arg(a, 0))
	if is_error(program) { return program }
	environment := send(state.environment_factory, "forNamespace:", arg(a, 1))
	if is_error(environment) { return environment }
	return send(program, "evalIn:", environment)
}
seed_eval_file :: proc "c" (m, r, a: Oop) -> Oop {
	context = runtime.default_context()
	path := as_bytes(arg(a, 0))
	if is_error(path) { return path }
	for ch in native_text(path) { if ch == 0 { return fail("invalidPath", "source path contains NUL") } }
	data, error := os.read_entire_file(native_text(path), context.allocator)
	if error != nil { return fail("sourceOpenFailed", "could not read source file") }
	defer delete(data)
	return send(r, "eval:", new_bytes(string(data)))
}
seed_syntax :: proc "c" (m, r, a: Oop) -> Oop { return state.syntax }
symbol_from_bytes :: proc "c" (m, r, a: Oop) -> Oop { return birnam_intern(arg(a, 0)) }

local_method :: proc "contextless" (target: Oop, name: string, entry: Entry, arity: int) {
	send(target, "define:as:", symbol(name), new_method(entry, arity))
}

@(export)
birnam_module_init_v1 :: proc "c" (method, world, args: Oop) -> Oop {
	if state.world != nil { return state.seed }
	GC_add_roots(rawptr(&state), rawptr(uintptr(&state)+size_of(State)))
	state.world = world
	state.nil_value = send(world, "at:", symbol("nil"))
	state.yes = send(world, "at:", symbol("true"))
	state.no = send(world, "at:", symbol("false"))
	state.error = send(world, "at:", symbol("Error"))
	state.object = send(world, "at:", symbol("Object"))
	state.integer_vt = new_behavior()
	state.text_vt = new_behavior()
	state.block_vt = new_behavior()
	state.environment_vt = new_behavior()
	state.interpreted_method_vt = send(birnam_method_behavior(), "delegated")
	state.cell_vt = new_behavior()
	state.literal_vt = new_behavior()
	state.name_vt = new_behavior()
	state.let_vt = new_behavior()
	state.send_vt = new_behavior()
	state.block_expr_vt = new_behavior()
	state.vector_expr_vt = new_behavior()
	state.sequence_vt = new_behavior()

	install(state.literal_vt, "evalIn:", literal_eval, 1)
	install(state.name_vt, "evalIn:", name_eval, 1)
	install(state.let_vt, "evalIn:", declaration_eval, 1)
	install(state.send_vt, "evalIn:", send_eval, 1)
	install(state.block_expr_vt, "evalIn:", block_expression_eval, 1)
	install(state.vector_expr_vt, "evalIn:", vector_expression_eval, 1)
	install(state.sequence_vt, "evalIn:", sequence_eval, 1)
	install(state.sequence_vt, "source", sequence_source, 0)
	syntax_accessor(state.literal_vt, "factory", offset_of(Literal, factory))
	syntax_accessor(state.literal_vt, "textBytes", offset_of(Literal, text))
	syntax_accessor(state.name_vt, "name", offset_of(Name, name))
	syntax_accessor(state.let_vt, "name", offset_of(Declaration, name))
	syntax_accessor(state.let_vt, "expression", offset_of(Declaration, expression))
	syntax_accessor(state.send_vt, "receiver", offset_of(Send, receiver))
	syntax_accessor(state.send_vt, "selector", offset_of(Send, selector))
	syntax_accessor(state.send_vt, "arguments", offset_of(Send, arguments))
	syntax_accessor(state.block_expr_vt, "parameters", offset_of(Block_Expression, parameters))
	syntax_accessor(state.block_expr_vt, "body", offset_of(Block_Expression, body))
	syntax_accessor(state.vector_expr_vt, "elements", offset_of(Vector_Expression, elements))
	syntax_accessor(state.sequence_vt, "forms", offset_of(Sequence, forms))
	install(state.environment_vt, "lookup:", environment_lookup, 1)
	install(state.environment_vt, "define:as:", environment_define, 2)
	install(state.environment_vt, "namespace", environment_namespace, 0)
	install(state.environment_vt, "parent", environment_parent, 0)
	install(state.environment_vt, "names", environment_names, 0)
	install(state.environment_vt, "forNamespace:", environment_for_namespace, 1)
	state.environment_factory = new_environment(world, state.nil_value, true)

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
	state.vector_factory = send(world, "at:", symbol("Vector"))
	install(birnam_vector_behavior(), "fromElements:", vector_from_elements, 1)
	install(birnam_vector_behavior(), "size", vector_size, 0)
	install(birnam_vector_behavior(), "at:", vector_at, 1)
	install(birnam_vector_behavior(), "at:put:", vector_put, 2)
	install(birnam_vector_behavior(), "append:", vector_append, 1)
	install(birnam_vector_behavior(), "concat:", vector_concat, 1)
	state.cell = allocate(state.cell_vt, size_of(Oop))
	(cast(^Oop)state.cell)^ = state.nil_value
	install(state.cell_vt, "with:", cell_with, 1)
	install(state.cell_vt, "value", cell_value, 0)
	install(state.cell_vt, "value:", cell_set, 1)

	state.block = allocate(state.block_vt, size_of(Function))
	empty_body := allocate(state.sequence_vt, size_of(Sequence))
	(cast(^Sequence)empty_body)^ = Sequence{arguments(), new_bytes("")}
	(cast(^Function)state.block)^ = Function{arguments(), empty_body, state.environment_factory}
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
	local_method(state.reflect, "defineMethod:on:as:", reflect_define, 3)
	install(birnam_method_behavior(), "valueOn:with:", method_value_on, 2)

	install(birnam_object_behavior(), "printString", object_print, 0)
	install(birnam_object_behavior(), "asInteger", conversion_error, 0)
	install(birnam_object_behavior(), "asBytes", conversion_error, 0)
	install(birnam_object_behavior(), "isNil", answer_false, 0)
	install(birnam_object_behavior(), "ifError:", identity_entry, 1)
	install(birnam_object_behavior(), "ifNotError:", success_apply, 1)
	local_method(state.nil_value, "isNil", answer_true, 0)
	local_method(state.nil_value, "printString", nil_print, 0)
	booleans := [2]Oop{state.yes, state.no}
	for value in booleans {
		local_method(value, "ifTrue:ifFalse:", if_true_false, 2)
		local_method(value, "ifTrue:", if_true, 1)
		local_method(value, "ifFalse:", if_false, 1)
		local_method(value, "not", bool_not, 0)
		local_method(value, "and:", bool_and, 1)
		local_method(value, "or:", bool_or, 1)
		local_method(value, "printString", bool_print, 0)
	}
	error_vt := send(state.error, "behavior")
	install(error_vt, "ifError:", error_recover, 1)
	install(error_vt, "ifNotError:", identity_entry, 1)
	install(error_vt, "printString", error_print, 0)
	state.system = send(state.object, "clone")
	local_method(state.system, "print:", system_print, 1)
	local_method(state.system, "println:", system_println, 1)
	state.seed = send(state.object, "clone")
	local_method(state.seed, "parse:", seed_parse, 1)
	local_method(state.seed, "eval:", seed_eval, 1)
	local_method(state.seed, "eval:in:", seed_eval_in, 2)
	local_method(state.seed, "evalFile:", seed_eval_file, 1)
	local_method(state.seed, "syntax", seed_syntax, 0)
	state.syntax = send(world, "new")
	local_method(state.syntax, "literal:bytes:", syntax_literal, 2)
	local_method(state.syntax, "name:", syntax_name, 1)
	local_method(state.syntax, "declaration:expression:", syntax_declaration, 2)
	local_method(state.syntax, "send:selector:arguments:", syntax_send, 3)
	local_method(state.syntax, "block:body:", syntax_block, 2)
	local_method(state.syntax, "vector:", syntax_vector, 1)
	local_method(state.syntax, "sequence:source:", syntax_sequence, 2)
	syntax_exports := [7]struct { name: string, behavior: Oop }{
		{"Literal", state.literal_vt}, {"Name", state.name_vt}, {"Declaration", state.let_vt},
		{"Send", state.send_vt}, {"Block", state.block_expr_vt}, {"Vector", state.vector_expr_vt}, {"Sequence", state.sequence_vt},
	}
	for pair in syntax_exports { send(state.syntax, "at:put:", symbol(pair.name), pair.behavior) }
	publish("Seed", state.seed)
	publish("Integer", state.integer)
	publish("String", state.text)
	publish("Symbol", state.symbol_factory)
	publish("Block", state.block)
	publish("Environment", state.environment_factory)
	publish("Reflect", state.reflect)
	publish("System", state.system)
	publish("Cell", state.cell)
	return state.seed
}
