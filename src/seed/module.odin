package seed

import "base:runtime"
import "core:os"
import "../abi"

// The bootstrap interpreter: parser, syntax families, and the Seed object.
// Runtime services are reached only through the world and messages.
State :: struct {
	seed, syntax: Oop,
	integer, text, symbol_factory, block, vector_factory, environment_factory: Oop,
	literal_vt, name_vt, let_vt, send_vt, block_expr_vt, vector_expr_vt, sequence_vt: Oop,
}
state: State

seed_parse :: proc "c" (m, r, a: Oop) -> Oop {
	context = runtime.default_context()
	return parse_source(arg(a, 0))
}
seed_eval :: proc "c" (m, r, a: Oop) -> Oop { return send(r, "eval:in:", arg(a, 0), abi.known.world) }
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

@(export)
birnam_module_init_v1 :: proc "c" (method, world, args: Oop) -> Oop {
	if state.seed != nil { return state.seed }
	abi.attach(world)
	// Captured once, as before the split: literal factories, Block, Vector, and
	// Environment are whatever the world binds when the interpreter loads.
	// Without runtime.dylib these lookups answer unknownName and the load fails.
	services := [6]struct { name: string, value: Oop }{
		{"Integer", nil}, {"String", nil}, {"Symbol", nil}, {"Block", nil}, {"Vector", nil}, {"Environment", nil},
	}
	for &service in services {
		service.value = send(world, "at:", symbol(service.name))
		if is_error(service.value) { return service.value }
	}
	object := send(world, "at:", symbol("Object"))
	if is_error(object) { return object }
	GC_add_roots(rawptr(&state), rawptr(uintptr(&state)+size_of(State)))
	state.integer, state.text, state.symbol_factory = services[0].value, services[1].value, services[2].value
	state.block, state.vector_factory, state.environment_factory = services[3].value, services[4].value, services[5].value
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

	seed := send(object, "clone")
	local_method(seed, "parse:", seed_parse, 1)
	local_method(seed, "eval:", seed_eval, 1)
	local_method(seed, "eval:in:", seed_eval_in, 2)
	local_method(seed, "evalFile:", seed_eval_file, 1)
	local_method(seed, "syntax", seed_syntax, 0)
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
	publish("Seed", seed)
	// Set last: a non-nil state.seed marks a completed, idempotent init.
	state.seed = seed
	return state.seed
}
