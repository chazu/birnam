package seed

// Each syntax family is an ordinary object behavior with its own evalIn:
// Method. There is no runtime switch over a native AST/value tag.
Literal :: struct { factory, text: Oop }
Name :: struct { name: Oop }
Declaration :: struct { name, expression: Oop }
Send :: struct { receiver, selector, arguments: Oop }
Block_Expression :: struct { parameters, body: Oop }
Vector_Expression :: struct { elements: Oop }
Sequence :: struct { forms, source: Oop }

literal :: proc "contextless" (factory, data: Oop) -> Oop {
	result := allocate(state.literal_vt, size_of(Literal))
	(cast(^Literal)result)^ = Literal{factory, data}
	return result
}
literal_eval :: proc "c" (m, r, a: Oop) -> Oop {
	node := cast(^Literal)r
	return send(node.factory, "fromBytes:", node.text)
}
name_eval :: proc "c" (m, r, a: Oop) -> Oop {
	return send(arg(a, 0), "lookup:", (cast(^Name)r).name)
}
declaration_eval :: proc "c" (m, r, a: Oop) -> Oop {
	node := cast(^Declaration)r
	env := arg(a, 0)
	value := send(node.expression, "evalIn:", env)
	result := send(env, "define:as:", node.name, value)
	if is_error(result) { return result }
	// Binding an Error retains it for inspection; it is not a discarded Error.
	return state.nil_value
}
send_eval :: proc "c" (m, r, a: Oop) -> Oop {
	node := cast(^Send)r
	env := arg(a, 0)
	receiver := send(node.receiver, "evalIn:", env)
	values := new_arguments(count(node.arguments))
	for i in 0..<count(node.arguments) {
		items(values)[i] = send(arg(node.arguments, i), "evalIn:", env)
	}
	// Errors are values; a receiver may explicitly handle one with ifError:.
	// Selection deliberately happens after every argument's evaluation.
	return birnam_send(receiver, node.selector, values)
}
block_expression_eval :: proc "c" (m, r, a: Oop) -> Oop {
	node := cast(^Block_Expression)r
	return send(state.block, "code:environment:parameters:", node.body, arg(a, 0), node.parameters)
}
vector_expression_eval :: proc "c" (m, r, a: Oop) -> Oop {
	node := cast(^Vector_Expression)r
	values := new_arguments(count(node.elements))
	for i in 0..<count(node.elements) {
		items(values)[i] = send(arg(node.elements, i), "evalIn:", arg(a, 0))
	}
	return send(state.vector_factory, "fromElements:", values)
}
sequence_eval :: proc "c" (m, r, a: Oop) -> Oop {
	forms := (cast(^Sequence)r).forms
	result := state.nil_value
	for i in 0..<count(forms) {
		result = send(arg(forms, i), "evalIn:", arg(a, 0))
		if i+1 < count(forms) && is_error(result) { return result }
	}
	return result
}
sequence_source :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Sequence)r).source }

// Syntax's native implementation supplies its own field accessors. This
// exposes the graph through messages without adding payload knowledge to Id.
syntax_field :: proc "c" (m, r, a: Oop) -> Oop {
	offset := (cast(^uintptr)m)^
	if header(r).payload_bytes < offset+size_of(Oop) { return fail("invalidSyntax", "receiver has an incompatible syntax representation") }
	return (cast(^Oop)(uintptr(r)+offset))^
}
syntax_accessor :: proc "contextless" (behavior: Oop, selector: string, offset: uintptr) {
	method := new_method(syntax_field, 0, size_of(uintptr))
	(cast(^uintptr)method)^ = offset
	send(behavior, "at:put:", symbol(selector), method)
}

// Owner-provided construction for authored CST consumers. These allocate the
// existing syntax representation; grammar and translation policy stay authored.
syntax_literal :: proc "c" (m, r, a: Oop) -> Oop {
	bytes := as_bytes(arg(a, 1))
	if is_error(bytes) { return bytes }
	return literal(arg(a, 0), bytes)
}
syntax_name :: proc "c" (m, r, a: Oop) -> Oop {
	result := allocate(state.name_vt, size_of(Name))
	(cast(^Name)result)^ = Name{arg(a, 0)}
	return result
}
syntax_declaration :: proc "c" (m, r, a: Oop) -> Oop {
	result := allocate(state.let_vt, size_of(Declaration))
	(cast(^Declaration)result)^ = Declaration{arg(a, 0), arg(a, 1)}
	return result
}
syntax_send :: proc "c" (m, r, a: Oop) -> Oop {
	if !valid_vector(arg(a, 2)) { return fail("typeMismatch", "Send arguments must be a Vector of syntax objects") }
	result := allocate(state.send_vt, size_of(Send))
	(cast(^Send)result)^ = Send{arg(a, 0), arg(a, 1), arg(a, 2)}
	return result
}
syntax_block :: proc "c" (m, r, a: Oop) -> Oop {
	if !valid_vector(arg(a, 0)) { return fail("typeMismatch", "Block parameters must be a Vector") }
	result := allocate(state.block_expr_vt, size_of(Block_Expression))
	(cast(^Block_Expression)result)^ = Block_Expression{arg(a, 0), arg(a, 1)}
	return result
}
syntax_vector :: proc "c" (m, r, a: Oop) -> Oop {
	if !valid_vector(arg(a, 0)) { return fail("typeMismatch", "Vector elements must be a Vector of syntax objects") }
	result := allocate(state.vector_expr_vt, size_of(Vector_Expression))
	(cast(^Vector_Expression)result)^ = Vector_Expression{arg(a, 0)}
	return result
}
syntax_sequence :: proc "c" (m, r, a: Oop) -> Oop {
	if !valid_vector(arg(a, 0)) { return fail("typeMismatch", "Sequence forms must be a Vector of syntax objects") }
	bytes := as_bytes(arg(a, 1))
	if is_error(bytes) { return bytes }
	result := allocate(state.sequence_vt, size_of(Sequence))
	(cast(^Sequence)result)^ = Sequence{arg(a, 0), bytes}
	return result
}
