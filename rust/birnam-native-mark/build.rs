fn main() {
    println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libbirnam_native_mark.dylib");
}
