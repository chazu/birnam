package seed

Binding :: struct { name, value: Oop, next: ^Binding }
Environment :: struct { namespace, parent: Oop, bindings: ^Binding, top_level: bool }

new_environment :: proc "contextless" (namespace, parent: Oop, top: bool) -> Oop {
	result := allocate(state.environment_vt, size_of(Environment))
	(cast(^Environment)result)^ = Environment{namespace=namespace, parent=parent, top_level=top}
	return result
}
environment_namespace :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Environment)r).namespace }
environment_parent :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Environment)r).parent }
environment_names :: proc "c" (m, r, a: Oop) -> Oop {
	names: List
	for row := (cast(^Environment)r).bindings; row != nil; row = row.next { append(&names, row.name) }
	return vector(names)
}
environment_lookup :: proc "c" (m, r, a: Oop) -> Oop {
	env := cast(^Environment)r
	name := arg(a, 0)
	if !env.top_level {
		for row := env.bindings; row != nil; row = row.next {
			if row.name == name { return row.value }
		}
	}
	if env.parent != state.nil_value { return send(env.parent, "lookup:", name) }
	return send(env.namespace, "at:", name)
}
environment_define :: proc "c" (m, r, a: Oop) -> Oop {
	env := cast(^Environment)r
	name, value := arg(a, 0), arg(a, 1)
	for row := env.bindings; row != nil; row = row.next {
		if row.name == name { return fail("duplicateBinding", "let cannot reassign a binding in the same scope") }
	}
	row := cast(^Binding)GC_malloc(size_of(Binding))
	row^ = Binding{name, value if !env.top_level else state.nil_value, env.bindings}
	env.bindings = row
	if env.top_level { return send(env.namespace, "at:put:", name, value) }
	return r
}
environment_for_namespace :: proc "c" (m, r, a: Oop) -> Oop {
	return new_environment(arg(a, 0), state.nil_value, true)
}

Function :: struct { parameters, code, environment: Oop }
function_call :: proc "contextless" (function: Function, values: Oop) -> Oop {
	if !valid_vector(values) { return fail("typeMismatch", "Block arguments must be a Vector") }
	if count(function.parameters) != count(values) { return fail("arityMismatch", "Block parameter count differs from argument count") }
	namespace := send(function.environment, "namespace")
	if is_error(namespace) { return namespace }
	env := new_environment(namespace, function.environment, false)
	for i in 0..<count(values) {
		defined := send(env, "define:as:", arg(function.parameters, i), arg(values, i))
		if is_error(defined) { return defined }
	}
	return send(function.code, "evalIn:", env)
}
block_create :: proc "c" (m, r, a: Oop) -> Oop {
	if !valid_vector(arg(a, 2)) { return fail("typeMismatch", "Block parameters must be a Vector") }
	result := allocate(state.block_vt, size_of(Function))
	(cast(^Function)result)^ = Function{arg(a, 2), arg(a, 0), arg(a, 1)}
	return result
}
block_value :: proc "c" (m, r, a: Oop) -> Oop { return function_call((cast(^Function)r)^, a) }
block_value_arguments :: proc "c" (m, r, a: Oop) -> Oop { return function_call((cast(^Function)r)^, arg(a, 0)) }
block_code :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Function)r).code }
block_parameters :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Function)r).parameters }
block_environment :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Function)r).environment }

interpreted_entry :: proc "c" (m, r, a: Oop) -> Oop {
	// This selected Method owns its captured code/environment. Invoking it does
	// not perform a second Block value/apply lookup.
	values := new_arguments(count(a)+1)
	items(values)[0] = r
	for i in 0..<count(a) { items(values)[i+1] = arg(a, i) }
	return function_call((cast(^Function)m)^, values)
}
method_from_block :: proc "contextless" (block: Oop) -> Oop {
	parameters := send(block, "parameters")
	if is_error(parameters) { return parameters }
	if !valid_vector(parameters) || count(parameters) == 0 {
		return fail("invalidMethod", "Method body needs a receiver parameter")
	}
	code := send(block, "code")
	if is_error(code) { return code }
	environment := send(block, "environment")
	if is_error(environment) { return environment }
	result := new_method(interpreted_entry, count(parameters)-1, size_of(Function))
	header(result).behavior = state.interpreted_method_vt
	(cast(^Function)result)^ = Function{parameters, code, environment}
	return result
}
reflect_method_from :: proc "c" (m, r, a: Oop) -> Oop { return method_from_block(arg(a, 0)) }
reflect_define :: proc "c" (m, r, a: Oop) -> Oop {
	defined := method_from_block(arg(a, 2))
	if is_error(defined) { return defined }
	return send(arg(a, 1), "define:as:", arg(a, 0), defined)
}
reflect_method_for :: proc "c" (m, r, a: Oop) -> Oop { return birnam_bind(arg(a, 1), arg(a, 0)) }
method_value_on :: proc "c" (m, r, a: Oop) -> Oop { return birnam_enter(r, arg(a, 0), arg(a, 1)) }
