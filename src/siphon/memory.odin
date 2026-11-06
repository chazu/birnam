package main

import "core:c/libc"

native_require :: proc "contextless" (condition: bool) {
	if !condition { libc.abort() }
}

foreign import gc "system:gc"
foreign gc {
	GC_init :: proc "c" () ---
	GC_set_all_interior_pointers :: proc "c" (enabled: i32) ---
	GC_set_finalize_on_demand :: proc "c" (enabled: i32) ---
	GC_malloc :: proc "c" (size: uintptr) -> rawptr ---
	GC_gcollect :: proc "c" () ---
	GC_get_version :: proc "c" () -> u32 ---
	GC_is_incremental_mode :: proc "c" () -> i32 ---
	GC_add_roots :: proc "c" (start, end: rawptr) ---
}

// Nothing in the dispatch header classifies the payload's language meaning.
Oop :: distinct rawptr
Entry :: proc "c" (method, receiver, arguments: Oop) -> Oop
Header :: struct {
	behavior: Oop,
	payload_bytes: uintptr,
	entry: Entry,
	arity: uintptr,
}
#assert(size_of(Header) == 4*size_of(uintptr))

header :: proc "contextless" (object: Oop) -> ^Header {
	return cast(^Header)(uintptr(object) - size_of(Header))
}

allocate :: proc "contextless" (behavior: Oop, size: uintptr) -> Oop {
	native_require(size < uintptr(max(int)) - size_of(Header))
	base := GC_malloc(size_of(Header) + max(size, 1))
	native_require(base != nil)
	h := cast(^Header)base
	h.behavior = behavior
	h.payload_bytes = size
	return Oop(uintptr(base) + size_of(Header))
}

// The seed call adapter owns this representation, shared with native.h.
// It is metadata for entering Methods, not a universal language value union.
Arguments :: struct { count: uintptr }

arguments :: proc "contextless" (values: ..Oop) -> Oop {
	object := allocate(roots.vector_vt, size_of(uintptr) + uintptr(len(values))*size_of(Oop))
	(cast(^Arguments)object).count = uintptr(len(values))
	items := cast([^]Oop)(uintptr(object) + size_of(uintptr))
	for value, index in values { items[index] = value }
	return object
}

argument_count :: proc "contextless" (object: Oop) -> (uintptr, bool) {
	if object == nil || header(object).payload_bytes < size_of(uintptr) { return 0, false }
	n := (cast(^Arguments)object).count
	return n, n == (header(object).payload_bytes - size_of(uintptr))/size_of(Oop) &&
		header(object).payload_bytes == size_of(uintptr) + n*size_of(Oop)
}

arg :: proc "contextless" (object: Oop, index: uintptr) -> Oop {
	return (cast([^]Oop)(uintptr(object) + size_of(uintptr)))[index]
}

method :: proc "contextless" (entry: Entry, arity: uintptr, capture_bytes: uintptr = 0) -> Oop {
	object := allocate(roots.method_vt, capture_bytes)
	header(object).entry = entry
	header(object).arity = arity
	return object
}

bytes :: proc "contextless" (value: string) -> Oop {
	object := allocate(roots.bytes_vt, uintptr(len(value)))
	copy((cast([^]u8)object)[:len(value)], transmute([]u8)value)
	return object
}

// Native Bytes/Symbol implementations and the CLI's byte adapter own this
// representation. General Id dispatch never interprets it.
byte_string :: proc "contextless" (object: Oop) -> string {
	return string((cast([^]u8)object)[:int(header(object).payload_bytes)])
}
