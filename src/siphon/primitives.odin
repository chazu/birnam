package main

// Storage floor for the authored Id module. These operations read and write
// the siphon's own Behavior/Namespace representations; definition, cloning,
// and reflection policy belongs to the authored methods built on them.
// Arguments are validated against those representations so a policy bug
// reports an Error instead of misinterpreting another object's payload.

delegates_to :: proc "contextless" (vt, ancestor: Oop) -> bool {
	for row := vt; row != nil; row = (cast(^Behavior)row).parent {
		if row == ancestor { return true }
	}
	return false
}

is_behavior :: proc "contextless" (object: Oop) -> bool {
	if object == nil || header(object).payload_bytes != size_of(Behavior) { return false }
	return delegates_to(header(object).behavior, roots.root_meta)
}

is_namespace :: proc "contextless" (object: Oop) -> bool {
	if object == nil || header(object).payload_bytes != size_of(Namespace) { return false }
	return delegates_to(header(object).behavior, roots.namespace_vt)
}

boolean :: proc "contextless" (value: bool) -> Oop {
	return roots.true_value if value else roots.false_value
}
optional :: proc "contextless" (value: Oop) -> Oop { return roots.nil_value if value == nil else value }

// Definition order: binding lists are newest-first.
binding_keys :: proc "contextless" (list: ^Binding) -> Oop {
	n: uintptr
	for row := list; row != nil; row = row.next { n += 1 }
	result := allocate(roots.vector_vt, size_of(uintptr) + n*size_of(Oop))
	(cast(^Arguments)result).count = n
	items := cast([^]Oop)(uintptr(result) + size_of(uintptr))
	i := n
	for row := list; row != nil; row = row.next { i -= 1; items[i] = row.key }
	return result
}

binding_remove :: proc "contextless" (list: ^^Binding, key: Oop) -> bool {
	for link := list; link^ != nil; link = &link^.next {
		if link^.key == key { link^ = link^.next; return true }
	}
	return false
}

not_behavior :: proc "contextless" () -> Oop { return failure("typeMismatch", "expected a vtable") }
not_namespace :: proc "contextless" () -> Oop { return failure("typeMismatch", "expected a Namespace") }

// Identity choice answering one operand unevaluated. Authored Id branches with
// it instead of sending ifTrue:ifFalse:, so definitions can still repair a
// replaced Boolean protocol.
prim_if :: proc "c" (m, r, a: Oop) -> Oop {
	if arg(a, 0) == roots.true_value { return arg(a, 1) }
	if arg(a, 0) == roots.false_value { return arg(a, 2) }
	return failure("nonBooleanCondition", "expected true or false")
}

prim_is_behavior :: proc "c" (m, r, a: Oop) -> Oop { return boolean(is_behavior(arg(a, 0))) }
prim_behavior_of :: proc "c" (m, r, a: Oop) -> Oop { return header(arg(a, 0)).behavior }
prim_set_behavior :: proc "c" (m, r, a: Oop) -> Oop {
	object, vt := arg(a, 0), arg(a, 1)
	if !is_behavior(vt) { return not_behavior() }
	// Only specialization: the existing payload keeps every implementation
	// that already interprets it.
	if !delegates_to(vt, header(object).behavior) {
		return failure("incompatibleBehavior", "new behavior must delegate to the current one")
	}
	header(object).behavior = vt
	// Not the object: an Error receiver would stop the caller's sequence.
	return vt
}
prim_new_behavior :: proc "c" (m, r, a: Oop) -> Oop {
	parent := arg(a, 0)
	if parent == roots.nil_value { return new_behavior(nil) }
	if !is_behavior(parent) { return not_behavior() }
	return new_behavior(parent)
}
prim_behavior_at :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_behavior(arg(a, 0)) { return not_behavior() }
	return optional(table_lookup(arg(a, 0), arg(a, 1)))
}
prim_behavior_includes :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_behavior(arg(a, 0)) { return not_behavior() }
	return boolean(table_lookup(arg(a, 0), arg(a, 1)) != nil)
}
prim_behavior_put :: proc "c" (m, r, a: Oop) -> Oop {
	// Raw storage: whether an entry must be callable is authored policy.
	vt := arg(a, 0)
	if !is_behavior(vt) { return not_behavior() }
	binding_put(&(cast(^Behavior)vt).bindings, arg(a, 1), arg(a, 2))
	return vt
}
prim_behavior_remove :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_behavior(arg(a, 0)) { return not_behavior() }
	return boolean(binding_remove(&(cast(^Behavior)arg(a, 0)).bindings, arg(a, 1)))
}
prim_selectors :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_behavior(arg(a, 0)) { return not_behavior() }
	return binding_keys((cast(^Behavior)arg(a, 0)).bindings)
}
prim_parent :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_behavior(arg(a, 0)) { return not_behavior() }
	return optional((cast(^Behavior)arg(a, 0)).parent)
}
prim_set_parent :: proc "c" (m, r, a: Oop) -> Oop {
	vt, parent := arg(a, 0), arg(a, 1)
	if !is_behavior(vt) { return not_behavior() }
	if parent == roots.nil_value { parent = nil } else if !is_behavior(parent) { return not_behavior() }
	// Native lookup and these walkers follow parent links without a bound.
	if parent != nil && delegates_to(parent, vt) {
		return failure("behaviorCycle", "parent would delegate back to this vtable")
	}
	(cast(^Behavior)vt).parent = parent
	return vt
}
prim_owned_by :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_behavior(arg(a, 0)) { return not_behavior() }
	owner := (cast(^Behavior)arg(a, 0)).owner
	return boolean(owner != nil && owner == arg(a, 1))
}
prim_set_owner :: proc "c" (m, r, a: Oop) -> Oop {
	vt, owner := arg(a, 0), arg(a, 1)
	if !is_behavior(vt) { return not_behavior() }
	if owner == roots.nil_value { owner = nil } else if header(owner).behavior != vt {
		return failure("incompatibleBehavior", "an owner must currently use the vtable")
	}
	(cast(^Behavior)vt).owner = owner
	return vt
}
prim_copy_object :: proc "c" (m, r, a: Oop) -> Oop { return clone_entry(m, arg(a, 0), arguments()) }
prim_is_callable :: proc "c" (m, r, a: Oop) -> Oop { return boolean(header(arg(a, 0)).entry != nil) }

prim_is_namespace :: proc "c" (m, r, a: Oop) -> Oop { return boolean(is_namespace(arg(a, 0))) }
prim_namespace_like :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_namespace(arg(a, 0)) { return not_namespace() }
	return namespace_new_entry(m, arg(a, 0), arguments())
}
prim_namespace_at :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_namespace(arg(a, 0)) { return not_namespace() }
	return namespace_at_entry(m, arg(a, 0), arguments(arg(a, 1)))
}
prim_namespace_put :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_namespace(arg(a, 0)) { return not_namespace() }
	binding_put(&(cast(^Namespace)arg(a, 0)).bindings, arg(a, 1), arg(a, 2))
	return arg(a, 0)
}
prim_namespace_includes :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_namespace(arg(a, 0)) { return not_namespace() }
	for row := (cast(^Namespace)arg(a, 0)).bindings; row != nil; row = row.next {
		if row.key == arg(a, 1) { return roots.true_value }
	}
	return roots.false_value
}
prim_namespace_remove :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_namespace(arg(a, 0)) { return not_namespace() }
	return boolean(binding_remove(&(cast(^Namespace)arg(a, 0)).bindings, arg(a, 1)))
}
prim_keys :: proc "c" (m, r, a: Oop) -> Oop {
	if !is_namespace(arg(a, 0)) { return not_namespace() }
	return binding_keys((cast(^Namespace)arg(a, 0)).bindings)
}

install_primitives :: proc "contextless" () -> Oop {
	vt := new_behavior(roots.object_vt)
	install(vt, "if:then:else:", prim_if, 3)
	install(vt, "isBehavior:", prim_is_behavior, 1)
	install(vt, "behaviorOf:", prim_behavior_of, 1)
	install(vt, "object:behavior:", prim_set_behavior, 2)
	install(vt, "newBehaviorWithParent:", prim_new_behavior, 1)
	install(vt, "behavior:at:", prim_behavior_at, 2)
	install(vt, "behavior:includes:", prim_behavior_includes, 2)
	install(vt, "behavior:at:put:", prim_behavior_put, 3)
	install(vt, "behavior:remove:", prim_behavior_remove, 2)
	install(vt, "selectorsOf:", prim_selectors, 1)
	install(vt, "parentOf:", prim_parent, 1)
	install(vt, "behavior:parent:", prim_set_parent, 2)
	install(vt, "behavior:isOwnedBy:", prim_owned_by, 2)
	install(vt, "behavior:owner:", prim_set_owner, 2)
	install(vt, "copyObject:", prim_copy_object, 1)
	install(vt, "isCallable:", prim_is_callable, 1)
	install(vt, "isNamespace:", prim_is_namespace, 1)
	install(vt, "newNamespaceLike:", prim_namespace_like, 1)
	install(vt, "namespace:at:", prim_namespace_at, 2)
	install(vt, "namespace:at:put:", prim_namespace_put, 3)
	install(vt, "namespace:includes:", prim_namespace_includes, 2)
	install(vt, "namespace:remove:", prim_namespace_remove, 2)
	install(vt, "keysOf:", prim_keys, 1)
	install(vt, "clone", identity_entry, 0)
	return allocate(vt, 0)
}
