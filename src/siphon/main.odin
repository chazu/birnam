package main

import "core:fmt"
import "core:os"

display_bytes :: proc(value: Oop) -> string {
	view := birnam_send(value, symbol("asBytes"), arguments())
	if birnam_send(view, roots.is_error, arguments()) == roots.true_value { return "<unprintable>" }
	return byte_string(view)
}

main :: proc() {
	world := birnam_bootstrap()
	if len(os.args) == 2 && os.args[1] == "--version" {
		fmt.printf("birnam siphon 0.1 (native seed; gc=%08x)\n", GC_get_version())
		return
	}
	if len(os.args) == 1 || (len(os.args) == 2 && os.args[1] == "--help") {
		fmt.println("usage: siphon [--version | --help | module.dylib | --eval source.birnam] ...")
		fmt.println("Load seed.dylib before --eval. Native modules and sources run in command-line order in one world.")
		return
	}
	siphon := birnam_send(world, symbol("at:"), arguments(symbol("Siphon")))
	for i := 1; i < len(os.args); i += 1 {
		path := os.args[i]
		result: Oop
		if path == "--eval" {
			if i+1 == len(os.args) { fmt.eprintln("--eval requires a source path"); os.exit(2) }
			i += 1
			path = os.args[i]
			seed := birnam_send(world, symbol("at:"), arguments(symbol("Seed")))
			result = birnam_send(seed, symbol("evalFile:"), arguments(bytes(path)))
		} else {
			result = birnam_send(siphon, symbol("load:"), arguments(bytes(path)))
		}
		if birnam_send(result, roots.is_error, arguments()) == roots.true_value {
			kind := birnam_send(result, symbol("kind"), arguments())
			description := birnam_send(result, symbol("description"), arguments())
			fmt.eprintf("%s: %s: %s\n", path, display_bytes(kind), display_bytes(description))
			os.exit(1)
		}
	}
}
