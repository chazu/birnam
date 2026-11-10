package abi

import "core:c/libc"
import "core:unicode/utf8"

// Shared native helpers, statically compiled into each module that imports
// this package. Mirrors the C headers; no Odin context crosses these boundaries.
Oop :: distinct rawptr
Entry :: proc "c" (method, receiver, arguments: Oop) -> Oop
Header :: struct { behavior: Oop, payload_bytes: uintptr, entry: Entry, arity: uintptr }
#assert(size_of(Header) == 4*size_of(uintptr))

foreign import host "system:c"
foreign host {
	birnam_send :: proc "c" (receiver, selector, args: Oop) -> Oop ---
	birnam_bind :: proc "c" (receiver, selector: Oop) -> Oop ---
	birnam_enter :: proc "c" (method, receiver, args: Oop) -> Oop ---
	birnam_intern :: proc "c" (name: Oop) -> Oop ---
	birnam_object_behavior :: proc "c" () -> Oop ---
	birnam_bytes_behavior :: proc "c" () -> Oop ---
	birnam_vector_behavior :: proc "c" () -> Oop ---
	birnam_method_behavior :: proc "c" () -> Oop ---
}
foreign import gc "system:gc"
foreign gc {
	GC_malloc :: proc "c" (size: uintptr) -> rawptr ---
	GC_add_roots :: proc "c" (start, end: rawptr) ---
}

// The well-known world objects these helpers read. Each module that imports
// this package has its own copy, filled by attach from that module's world.
Symbol_Cache :: struct { name: Oop, value: Oop, next: ^Symbol_Cache }
Known :: struct {
	world, nil_value, yes, no, error: Oop,
	symbols: ^Symbol_Cache,
}
known: Known

// Idempotent: the first call roots the struct and reads the well-known objects.
attach :: proc "contextless" (world: Oop) {
	if known.world != nil { return }
	GC_add_roots(rawptr(&known), rawptr(uintptr(&known)+size_of(Known)))
	known.world = world
	known.nil_value = send(world, "at:", symbol("nil"))
	known.yes = send(world, "at:", symbol("true"))
	known.no = send(world, "at:", symbol("false"))
	known.error = send(world, "at:", symbol("Error"))
}

header :: proc "contextless" (object: Oop) -> ^Header {
	return cast(^Header)(uintptr(object)-size_of(Header))
}
allocate :: proc "contextless" (behavior: Oop, size: uintptr) -> Oop {
	if size > uintptr(max(int))-size_of(Header) { libc.abort() }
	h := cast(^Header)GC_malloc(size_of(Header)+max(size, 1))
	if h == nil { libc.abort() }
	h.behavior, h.payload_bytes = behavior, size
	return Oop(uintptr(h)+size_of(Header))
}
new_bytes :: proc "contextless" (text: string) -> Oop {
	result := allocate(birnam_bytes_behavior(), uintptr(len(text)))
	copy((cast([^]u8)result)[:len(text)], transmute([]u8)text)
	return result
}
native_text :: proc "contextless" (object: Oop) -> string {
	return string((cast([^]u8)object)[:int(header(object).payload_bytes)])
}
new_arguments :: proc "contextless" (count: int) -> Oop {
	result := allocate(birnam_vector_behavior(), size_of(uintptr)+uintptr(count)*size_of(Oop))
	(cast(^uintptr)result)^ = uintptr(count)
	return result
}
count :: proc "contextless" (vector: Oop) -> int { return int((cast(^uintptr)vector)^) }
items :: proc "contextless" (vector: Oop) -> [^]Oop { return cast([^]Oop)(uintptr(vector)+size_of(uintptr)) }
arg :: proc "contextless" (vector: Oop, index: int) -> Oop { return items(vector)[index] }
arguments :: proc "contextless" (values: ..Oop) -> Oop {
	result := new_arguments(len(values))
	for value, i in values { items(result)[i] = value }
	return result
}
new_method :: proc "contextless" (entry: Entry, arity: int, capture_bytes: uintptr = 0) -> Oop {
	result := allocate(birnam_method_behavior(), capture_bytes)
	header(result).entry, header(result).arity = entry, uintptr(arity)
	return result
}

// Parser/build scratch lists use GC storage: native allocator arrays would
// otherwise hide the only references to partially constructed syntax objects.
Link :: struct { value: Oop, next: ^Link }
List :: struct { head, tail: ^Link, size: int }
append :: proc "contextless" (list: ^List, value: Oop) {
	link := cast(^Link)GC_malloc(size_of(Link))
	if link == nil { libc.abort() }
	link.value = value
	if list.tail == nil { list.head = link } else { list.tail.next = link }
	list.tail = link
	list.size += 1
}
vector :: proc "contextless" (list: List) -> Oop {
	result := new_arguments(list.size)
	i := 0
	for row := list.head; row != nil; row = row.next { items(result)[i] = row.value; i += 1 }
	return result
}

symbol :: proc "contextless" (name: string) -> Oop {
	for row := known.symbols; row != nil; row = row.next {
		if native_text(row.name) == name { return row.value }
	}
	name_bytes := new_bytes(name)
	result := birnam_intern(name_bytes)
	row := cast(^Symbol_Cache)GC_malloc(size_of(Symbol_Cache))
	if row == nil { libc.abort() }
	row^ = Symbol_Cache{name_bytes, result, known.symbols}
	known.symbols = row
	return result
}
send :: proc "contextless" (receiver: Oop, selector: string, args: ..Oop) -> Oop {
	return birnam_send(receiver, symbol(selector), arguments(..args))
}
is_error :: proc "contextless" (value: Oop) -> bool {
	return send(value, "isError") == known.yes
}
fail :: proc "contextless" (kind, message: string) -> Oop {
	return send(known.error, "kind:message:", symbol(kind), new_bytes(message))
}
publish :: proc "contextless" (name: string, value: Oop) {
	send(known.world, "at:put:", symbol(name), value)
}
install :: proc "contextless" (behavior: Oop, name: string, entry: Entry, arity: int) {
	send(behavior, "at:put:", symbol(name), new_method(entry, arity))
}
local_method :: proc "contextless" (target: Oop, name: string, entry: Entry, arity: int) {
	send(target, "define:as:", symbol(name), new_method(entry, arity))
}
new_behavior :: proc "contextless" () -> Oop {
	return send(birnam_object_behavior(), "delegated")
}

// These checks are confined to native representation owners/adapters, never
// the generic expression evaluator. Inherited compatible representations work.
has_behavior :: proc "contextless" (object, expected: Oop) -> bool {
	for vt := header(object).behavior; vt != known.nil_value; vt = send(vt, "parent") {
		if vt == expected { return true }
		if is_error(vt) { return false }
	}
	return false
}
as_bytes :: proc "contextless" (object: Oop) -> Oop {
	result := send(object, "asBytes")
	if is_error(result) { return result }
	if !has_behavior(result, birnam_bytes_behavior()) { return fail("typeMismatch", "asBytes must return native Bytes") }
	return result
}
valid_vector :: proc "contextless" (object: Oop) -> bool {
	if !has_behavior(object, birnam_vector_behavior()) || header(object).payload_bytes < size_of(uintptr) { return false }
	n := (cast(^uintptr)object)^
	return n == (header(object).payload_bytes-size_of(uintptr))/size_of(Oop) && header(object).payload_bytes == size_of(uintptr)+n*size_of(Oop)
}

// Pure text helpers shared by the runtime's Integer/String services and the
// seed parser's literal checks.
utf8_error_offset :: proc "contextless" (text: string) -> int {
	i := 0
	for i < len(text) {
		r, width := utf8.decode_rune(text[i:])
		if r == utf8.RUNE_ERROR && width == 1 { return i }
		i += width
	}
	return -1
}
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
