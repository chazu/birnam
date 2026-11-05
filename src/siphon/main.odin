package main

import "core:fmt"
import "core:os"

main :: proc() {
	world := birnam_bootstrap()
	if len(os.args) == 2 && os.args[1] == "--version" {
		fmt.printf("birnam siphon 0.1 (native seed; gc=%08x)\n", GC_get_version())
		return
	}
	if len(os.args) == 1 || (len(os.args) == 2 && os.args[1] == "--help") {
		fmt.println("usage: siphon [--version | --help | module.dylib ...]")
		fmt.println("Loads native Birnam modules into one bootstrap Id world. Source/image evaluation is not implemented yet.")
		return
	}
	siphon := birnam_send(world, symbol("at:"), arguments(symbol("Siphon")))
	for path in os.args[1:] {
		result := birnam_send(siphon, symbol("load:"), arguments(bytes(path)))
		if birnam_send(result, roots.is_error, arguments()) == roots.true_value {
			kind := birnam_send(result, symbol("kind"), arguments())
			description := birnam_send(result, symbol("description"), arguments())
			fmt.eprintf("%s: %s: %s\n", path, byte_string(kind), byte_string(description))
			os.exit(1)
		}
	}
}
