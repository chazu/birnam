fn main() {
    println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libbirnam_string_builder.dylib");
}
