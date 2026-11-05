package main

import "base:runtime"
import "core:dynlib"

Siphon_Data :: struct { world: Oop, modules: ^Module_Link }
Module_Link :: struct { value: Oop, next: ^Module_Link }
Module_Data :: struct { handle: dynlib.Library, name, initializer, result: Oop }

siphon_world_entry :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Siphon_Data)r).world }
siphon_load_entry :: proc "c" (m, r, a: Oop) -> Oop {
	// The path's implementation owns its native bytes and loader operation.
	return birnam_send(arg(a, 0), roots.load_into, arguments(r))
}
siphon_retain_entry :: proc "c" (m, r, a: Oop) -> Oop {
	state := cast(^Siphon_Data)r
	link := cast(^Module_Link)GC_malloc(size_of(Module_Link))
	native_require(link != nil)
	link^ = Module_Link{arg(a, 0), state.modules}
	state.modules = link
	return r
}
siphon_modules_entry :: proc "c" (m, r, a: Oop) -> Oop {
	state := cast(^Siphon_Data)r
	n: uintptr
	for link := state.modules; link != nil; link = link.next { n += 1 }
	result := allocate(roots.vector_vt, size_of(uintptr) + n*size_of(Oop))
	(cast(^Arguments)result).count = n
	items := cast([^]Oop)(uintptr(result) + size_of(uintptr))
	i := n
	for link := state.modules; link != nil; link = link.next { i -= 1; items[i] = link.value }
	return result
}
module_name_entry :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Module_Data)r).name }
module_result_entry :: proc "c" (m, r, a: Oop) -> Oop { return (cast(^Module_Data)r).result }

bytes_load_entry :: proc "c" (m, r, a: Oop) -> Oop {
	context = runtime.default_context()
	path := byte_string(r)
	if len(path) == 0 { return failure("invalidModulePath", "module path is empty") }
	for ch in path { if ch == 0 { return failure("invalidModulePath", "module path contains NUL") } }
	lib, ok := dynlib.load_library(path)
	if !ok { return failure("moduleOpenFailed", dynlib.last_error()) }
	module := allocate(roots.module_vt, size_of(Module_Data))
	data := cast(^Module_Data)module
	data.handle = lib
	data.name = r
	data.result = roots.nil_value
	data.initializer = roots.nil_value
	siphon := arg(a, 0)
	// Retain every successful dlopen. OS constructors and failed initializers
	// may already have published native Methods; no unmapping is safe here.
	retained := birnam_send(siphon, symbol("retain:"), arguments(module))
	if birnam_send(retained, roots.is_error, arguments()) == roots.true_value {
		// The custom retention operation has arbitrary effects. Never dlclose code
		// which it may have retained or invoked.
		return retained
	}
	address, found := dynlib.symbol_address(lib, "birnam_module_init_v1")
	if !found {
		data.result = failure("moduleEntryMissing", dynlib.last_error())
		return data.result
	}
	data.initializer = method(cast(Entry)address, 0)
	world := birnam_send(siphon, symbol("world"), arguments())
	if birnam_send(world, roots.is_error, arguments()) == roots.true_value { return world }
	result := birnam_enter(data.initializer, world, arguments())
	data.result = result
	if birnam_send(result, roots.is_error, arguments()) == roots.true_value { return result }
	return module
}
