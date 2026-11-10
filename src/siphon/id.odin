package main

Binding :: struct { key, value: Oop, next: ^Binding }
// owner is the single object a private layer was made for; nil once shared.
Behavior :: struct { parent: Oop, bindings: ^Binding, owner: Oop }
Namespace :: struct { bindings: ^Binding }
Interned :: struct { symbol: Oop, next: ^Interned }
Symbols :: struct { names: ^Interned }
Error_Data :: struct { kind, description: Oop }

Roots :: struct {
	world, object_vt, root_meta, bytes_vt, symbol_vt, vector_vt, method_vt: Oop,
	error_vt, namespace_vt, symbols_vt, siphon_vt, module_vt, gc_vt, singleton_vt: Oop,
	nil_value, true_value, false_value, symbols, siphon, collector: Oop,
	lookup, is_error, intern_name, as_selector, load_into: Oop,
}
roots: Roots

new_behavior :: proc "contextless" (parent: Oop) -> Oop {
	object := allocate(roots.root_meta, size_of(Behavior))
	(cast(^Behavior)object).parent = parent
	return object
}

// Another object now uses this behavior, so in-place local edits would leak.
share_behavior :: proc "contextless" (vt: Oop) { (cast(^Behavior)vt).owner = nil }

binding_at :: proc "contextless" (list: ^Binding, key: Oop) -> Oop {
	for row := list; row != nil; row = row.next {
		if row.key == key { return row.value }
	}
	return nil
}

binding_put :: proc "contextless" (list: ^^Binding, key, value: Oop) {
	for row := list^; row != nil; row = row.next {
		if row.key == key { row.value = value; return }
	}
	row := cast(^Binding)GC_malloc(size_of(Binding))
	native_require(row != nil)
	row^ = Binding{key, value, list^}
	list^ = row
}

// Used directly only for bootstrap and the single reflective root case.
table_lookup :: proc "contextless" (vt, selector: Oop) -> Oop {
	return binding_at((cast(^Behavior)vt).bindings, selector)
}

symbol :: proc "contextless" (name: string) -> Oop {
	s := cast(^Symbols)roots.symbols
	for row := s.names; row != nil; row = row.next {
		if byte_string(row.symbol) == name { return row.symbol }
	}
	object := bytes(name)
	header(object).behavior = roots.symbol_vt
	row := cast(^Interned)GC_malloc(size_of(Interned))
	native_require(row != nil)
	row^ = Interned{object, s.names}
	s.names = row
	return object
}

failure :: proc "contextless" (kind, description: string) -> Oop {
	object := allocate(roots.error_vt, size_of(Error_Data))
	(cast(^Error_Data)object)^ = Error_Data{symbol(kind), bytes(description)}
	return object
}

install :: proc "contextless" (vt: Oop, name: string, entry: Entry, arity: uintptr) {
	binding_put(&(cast(^Behavior)vt).bindings, symbol(name), method(entry, arity))
}

publish :: proc "contextless" (name: string, value: Oop) {
	binding_put(&(cast(^Namespace)roots.world).bindings, symbol(name), value)
}

@(export)
birnam_object_behavior :: proc "c" () -> Oop { return roots.object_vt }
@(export)
birnam_bytes_behavior :: proc "c" () -> Oop { return roots.bytes_vt }
@(export)
birnam_vector_behavior :: proc "c" () -> Oop { return roots.vector_vt }
@(export)
birnam_method_behavior :: proc "c" () -> Oop { return roots.method_vt }

@(export)
birnam_intern :: proc "c" (name: Oop) -> Oop {
	return birnam_send(roots.symbols, roots.intern_name, arguments(name))
}

@(export)
birnam_bind :: proc "c" (receiver, selector: Oop) -> Oop {
	if receiver == nil || selector == nil { return failure("invalidCall", "null is not a language Oop") }
	vt := header(receiver).behavior
	if vt == nil { return failure("invalidBehavior", "receiver has no behavior") }
	if receiver == roots.root_meta && selector == roots.lookup {
		found := table_lookup(vt, selector)
		if found == nil { return failure("missingRootLookup", "root lookup entry is absent") }
		return found
	}
	return birnam_send(vt, roots.lookup, arguments(selector, receiver))
}

@(export)
birnam_enter :: proc "c" (selected, receiver, args: Oop) -> Oop {
	if selected == nil || receiver == nil { return failure("invalidCall", "null Method or receiver") }
	h := header(selected)
	if h.entry == nil { return failure("notMethod", "selected object has no native entry") }
	n, valid := argument_count(args)
	if !valid { return failure("invalidArguments", "invalid native argument Vector") }
	if n != h.arity { return failure("arityMismatch", "argument count differs from Method arity") }
	for i in 0..<n {
		if arg(args, i) == nil { return failure("invalidArguments", "null argument is not a language Oop") }
	}
	result := h.entry(selected, receiver, args)
	if result == nil { return failure("invalidResult", "native Method returned null") }
	return result
}

@(export)
birnam_send :: proc "c" (receiver, selector, args: Oop) -> Oop {
	selected := birnam_bind(receiver, selector)
	if header(selected).entry == nil {
		// In the valid seed graph, isError is always a callable Method.
		// Inspect through behavior; there is no native Error discriminator.
		if birnam_send(selected, roots.is_error, arguments()) == roots.true_value { return selected }
		return failure("notMethod", "lookup did not select a Method")
	}
	return birnam_enter(selected, receiver, args)
}

lookup_entry :: proc "c" (m, receiver, args: Oop) -> Oop {
	vt := cast(^Behavior)receiver
	selector, original := arg(args, 0), arg(args, 1)
	if found := binding_at(vt.bindings, selector); found != nil { return found }
	if vt.parent != nil { return birnam_send(vt.parent, roots.lookup, arguments(selector, original)) }
	return failure("messageNotUnderstood", "selector is absent from the behavior chain")
}

behavior_entry :: proc "c" (m, r, a: Oop) -> Oop { return header(r).behavior }
false_entry :: proc "c" (m, r, a: Oop) -> Oop { return roots.false_value }
true_entry :: proc "c" (m, r, a: Oop) -> Oop { return roots.true_value }
identity_entry :: proc "c" (m, r, a: Oop) -> Oop { return r }
same_entry :: proc "c" (m, r, a: Oop) -> Oop {
	return roots.true_value if r == arg(a, 0) else roots.false_value
}

clone_entry :: proc "c" (m, r, a: Oop) -> Oop {
	h := header(r)
	share_behavior(h.behavior)
	copy_object := allocate(h.behavior, h.payload_bytes)
	copy((cast([^]u8)copy_object)[:int(h.payload_bytes)], (cast([^]u8)r)[:int(h.payload_bytes)])
	header(copy_object).entry = h.entry
	header(copy_object).arity = h.arity
	return copy_object
}

define_entry :: proc "c" (m, r, a: Oop) -> Oop {
	selected := arg(a, 1)
	if header(selected).entry == nil { return failure("notMethod", "definition requires a Method") }
	// Never mutate a behavior which may be shared by a clone.
	private := new_behavior(header(r).behavior)
	binding_put(&(cast(^Behavior)private).bindings, arg(a, 0), selected)
	(cast(^Behavior)private).owner = r
	header(r).behavior = private
	return r
}

delegated_entry :: proc "c" (m, r, a: Oop) -> Oop { return new_behavior(r) }
parent_entry :: proc "c" (m, r, a: Oop) -> Oop {
	parent := (cast(^Behavior)r).parent
	return roots.nil_value if parent == nil else parent
}
put_method_entry :: proc "c" (m, r, a: Oop) -> Oop {
	selected := arg(a, 1)
	if header(selected).entry == nil { return failure("notMethod", "vtable requires a Method") }
	binding_put(&(cast(^Behavior)r).bindings, arg(a, 0), selected)
	return r
}
vtable_clone_entry :: proc "c" (m, r, a: Oop) -> Oop {
	old := cast(^Behavior)r
	result := new_behavior(old.parent)
	share_behavior(header(r).behavior)
	header(result).behavior = header(r).behavior
	for row := old.bindings; row != nil; row = row.next {
		binding_put(&(cast(^Behavior)result).bindings, row.key, row.value)
	}
	return result
}

namespace_at_entry :: proc "c" (m, r, a: Oop) -> Oop {
	found := binding_at((cast(^Namespace)r).bindings, arg(a, 0))
	if found == nil { return failure("unknownName", "namespace has no such binding") }
	return found
}
namespace_put_entry :: proc "c" (m, r, a: Oop) -> Oop {
	binding_put(&(cast(^Namespace)r).bindings, arg(a, 0), arg(a, 1))
	return r
}
namespace_at_if_absent_entry :: proc "c" (m, r, a: Oop) -> Oop {
	found := binding_at((cast(^Namespace)r).bindings, arg(a, 0))
	if found != nil { return found }
	return birnam_send(arg(a, 1), symbol("value"), arguments())
}

namespace_clone_entry :: proc "c" (m, r, a: Oop) -> Oop {
	share_behavior(header(r).behavior)
	result := allocate(header(r).behavior, size_of(Namespace))
	for row := (cast(^Namespace)r).bindings; row != nil; row = row.next {
		binding_put(&(cast(^Namespace)result).bindings, row.key, row.value)
	}
	return result
}
namespace_new_entry :: proc "c" (m, r, a: Oop) -> Oop {
	share_behavior(header(r).behavior)
	return allocate(header(r).behavior, size_of(Namespace))
}

intern_entry :: proc "c" (m, r, a: Oop) -> Oop {
	// The name object interprets its own payload through its current behavior.
	return birnam_send(arg(a, 0), roots.as_selector, arguments())
}
bytes_intern_entry :: proc "c" (m, r, a: Oop) -> Oop {
	return symbol(byte_string(r))
}
error_kind_entry :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Error_Data)r).kind }
error_description_entry :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Error_Data)r).description }
error_return_entry :: proc "c" (m, r, a: Oop) -> Oop { return r }
error_make_entry :: proc "c" (m, r, a: Oop) -> Oop {
	object := allocate(roots.error_vt, size_of(Error_Data))
	(cast(^Error_Data)object)^ = Error_Data{arg(a, 0), arg(a, 1)}
	return object
}

// A missing ordinary message on Error resolves to an arity-matched propagator.
// No Error branch is installed in the application's sending path.
error_lookup_entry :: proc "c" (m, r, a: Oop) -> Oop {
	selector := arg(a, 0)
	// Only explicitly installed Error protocol operations handle the Error.
	// An unrelated method on Object must not swallow it through inheritance.
	if found := table_lookup(r, selector); found != nil { return found }
	name := byte_string(selector)
	arity: uintptr
	for ch in name { if ch == ':' { arity += 1 } }
	if arity == 0 && len(name) > 0 {
		ch := name[0]
		if !(ch >= 'a' && ch <= 'z') && !(ch >= 'A' && ch <= 'Z') && ch != '_' { arity = 1 }
	}
	return method(error_return_entry, arity)
}

gc_collect_entry :: proc "c" (m, r, a: Oop) -> Oop { GC_gcollect(); return r }

@(export)
birnam_bootstrap :: proc "c" () -> Oop {
	if roots.world != nil { return roots.world }
	GC_set_all_interior_pointers(1)
	GC_set_finalize_on_demand(1)
	GC_init()
	native_require(GC_is_incremental_mode() == 0)
	GC_add_roots(rawptr(&roots), rawptr(uintptr(&roots) + size_of(Roots)))
	roots.root_meta = new_behavior(nil)
	header(roots.root_meta).behavior = roots.root_meta
	roots.object_vt = new_behavior(nil)
	(cast(^Behavior)roots.root_meta).parent = roots.object_vt
	roots.bytes_vt = new_behavior(roots.object_vt)
	roots.symbol_vt = new_behavior(roots.bytes_vt)
	roots.vector_vt = new_behavior(roots.object_vt)
	roots.method_vt = new_behavior(roots.object_vt)
	roots.namespace_vt = new_behavior(roots.object_vt)
	roots.symbols_vt = new_behavior(roots.object_vt)
	roots.error_vt = new_behavior(roots.object_vt)
	roots.siphon_vt = new_behavior(roots.object_vt)
	roots.module_vt = new_behavior(roots.object_vt)
	roots.gc_vt = new_behavior(roots.object_vt)
	roots.singleton_vt = new_behavior(roots.object_vt)
	roots.nil_value = allocate(roots.singleton_vt, 0)
	roots.true_value = allocate(roots.singleton_vt, 0)
	roots.false_value = allocate(roots.singleton_vt, 0)
	roots.symbols = allocate(roots.symbols_vt, size_of(Symbols))
	roots.world = allocate(roots.namespace_vt, size_of(Namespace))
	roots.lookup = symbol("lookup:for:")
	roots.is_error = symbol("isError")
	roots.intern_name = symbol("intern:")
	roots.as_selector = symbol("intern")
	roots.load_into = symbol("loadInto:")
	install(roots.root_meta, "lookup:for:", lookup_entry, 2)
	install(roots.root_meta, "delegated", delegated_entry, 0)
	install(roots.root_meta, "parent", parent_entry, 0)
	install(roots.root_meta, "at:put:", put_method_entry, 2)
	install(roots.root_meta, "clone", vtable_clone_entry, 0)
	install(roots.object_vt, "behavior", behavior_entry, 0)
	install(roots.object_vt, "clone", clone_entry, 0)
	install(roots.object_vt, "define:as:", define_entry, 2)
	install(roots.object_vt, "isError", false_entry, 0)
	install(roots.object_vt, "==", same_entry, 1)
	install(roots.namespace_vt, "at:", namespace_at_entry, 1)
	install(roots.namespace_vt, "at:put:", namespace_put_entry, 2)
	install(roots.namespace_vt, "at:ifAbsent:", namespace_at_if_absent_entry, 2)
	install(roots.namespace_vt, "clone", namespace_clone_entry, 0)
	install(roots.namespace_vt, "new", namespace_new_entry, 0)
	install(roots.symbols_vt, "intern:", intern_entry, 1)
	install(roots.symbols_vt, "clone", identity_entry, 0)
	install(roots.bytes_vt, "intern", bytes_intern_entry, 0)
	install(roots.bytes_vt, "asBytes", identity_entry, 0)
	install(roots.symbol_vt, "intern", identity_entry, 0)
	install(roots.symbol_vt, "clone", identity_entry, 0)
	install(roots.singleton_vt, "clone", identity_entry, 0)
	install(roots.error_vt, "isError", true_entry, 0)
	install(roots.error_vt, "kind", error_kind_entry, 0)
	install(roots.error_vt, "description", error_description_entry, 0)
	install(roots.error_vt, "kind:message:", error_make_entry, 2)
	install(roots.error_vt, "behavior", behavior_entry, 0)
	install(roots.error_vt, "define:as:", define_entry, 2)
	install(roots.error_vt, "==", same_entry, 1)
	install(roots.error_vt, "clone", identity_entry, 0)
	error_meta := new_behavior(roots.root_meta)
	install(error_meta, "lookup:for:", error_lookup_entry, 2)
	header(roots.error_vt).behavior = error_meta
	install(roots.gc_vt, "collect", gc_collect_entry, 0)
	install(roots.gc_vt, "clone", identity_entry, 0)
	roots.collector = allocate(roots.gc_vt, 0)
	roots.siphon = allocate(roots.siphon_vt, size_of(Siphon_Data))
	(cast(^Siphon_Data)roots.siphon).world = roots.world
	install(roots.siphon_vt, "world", siphon_world_entry, 0)
	install(roots.siphon_vt, "load:", siphon_load_entry, 1)
	install(roots.siphon_vt, "retain:", siphon_retain_entry, 1)
	install(roots.siphon_vt, "modules", siphon_modules_entry, 0)
	install(roots.siphon_vt, "clone", identity_entry, 0)
	install(roots.bytes_vt, "loadInto:", bytes_load_entry, 1)
	install(roots.module_vt, "name", module_name_entry, 0)
	install(roots.module_vt, "result", module_result_entry, 0)
	install(roots.module_vt, "clone", identity_entry, 0)
	publish("Object", allocate(roots.object_vt, 0))
	publish("Bytes", bytes(""))
	publish("Vector", arguments())
	publish("Method", allocate(roots.method_vt, 0))
	publish("Error", failure("error", "Error prototype"))
	publish("Vtable", roots.root_meta)
	publish("Symbols", roots.symbols)
	publish("Namespace", roots.world)
	publish("Siphon", roots.siphon)
	publish("GC", roots.collector)
	publish("IdPrimitives", install_primitives())
	publish("nil", roots.nil_value)
	publish("true", roots.true_value)
	publish("false", roots.false_value)
	return roots.world
}
