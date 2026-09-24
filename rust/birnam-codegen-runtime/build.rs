fn main() {
    println!(
        "cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libbirnam_codegen_runtime.dylib"
    );
}
